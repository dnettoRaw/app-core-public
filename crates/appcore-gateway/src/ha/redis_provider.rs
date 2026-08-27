// =============================================================================
//        #######
//     ###       ###     F: redis_provider.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Concrete bounded Redis connection and script execution boundary.

use super::redis_keys::RedisGatewayKeys;
use super::redis_scripts::{
    CHECK_SCHEMA, STATUS_CAPACITY, STATUS_CONFLICT, STATUS_EXPIRED, STATUS_INVALID, STATUS_OK,
    STATUS_STALE, STATUS_UNSUPPORTED_SCHEMA,
};
use super::{
    GatewayRegistryError, GatewayRegistryResult, RedisGatewayCredential,
    RedisGatewayRegistryConfig, GATEWAY_HA_SCHEMA_V2,
};
use redis::aio::MultiplexedConnection;
use redis::{AsyncConnectionConfig, FromRedisValue};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, RwLock, Semaphore};

/// Official Redis implementation of the shared Gateway HA registry.
pub struct RedisGatewayRegistryProvider {
    pub(crate) config: RedisGatewayRegistryConfig,
    pub(crate) keys: RedisGatewayKeys,
    client: redis::Client,
    credential: RedisGatewayCredential,
    connection: RwLock<Option<MultiplexedConnection>>,
    reconnect_lock: Mutex<()>,
    slots: Arc<Semaphore>,
}

impl Debug for RedisGatewayRegistryProvider {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedisGatewayRegistryProvider")
            .field("config", &self.config)
            .field("credential", &"REDACTED")
            .finish_non_exhaustive()
    }
}

impl RedisGatewayRegistryProvider {
    /// Connects, authenticates and verifies the provider schema.
    pub async fn connect(
        config: RedisGatewayRegistryConfig,
        credential: RedisGatewayCredential,
    ) -> GatewayRegistryResult<Self> {
        let client = redis::Client::open(config.endpoint())
            .map_err(|_| GatewayRegistryError::InvalidContract)?;
        let provider = Self {
            keys: RedisGatewayKeys::new(config.namespace()),
            slots: Arc::new(Semaphore::new(config.max_concurrency())),
            config,
            client,
            credential,
            connection: RwLock::new(None),
            reconnect_lock: Mutex::new(()),
        };
        provider.reconnect().await?;
        Ok(provider)
    }

    /// Explicitly replaces a failed connection after lifecycle isolation.
    ///
    /// Registry mutations are never retried implicitly because command
    /// completion can be ambiguous after a transport failure.
    pub async fn reconnect(&self) -> GatewayRegistryResult<()> {
        let _reconnect =
            tokio::time::timeout(self.config.operation_timeout(), self.reconnect_lock.lock())
                .await
                .map_err(|_| GatewayRegistryError::CapacityExceeded)?;
        let mut connection = self.open_authenticated_connection().await?;
        self.verify_schema(&mut connection).await?;
        let mut current =
            tokio::time::timeout(self.config.operation_timeout(), self.connection.write())
                .await
                .map_err(|_| GatewayRegistryError::Unavailable)?;
        *current = Some(connection);
        Ok(())
    }

    pub(crate) async fn script<T>(
        &self,
        source: &str,
        keys: &[String],
        arguments: &[String],
    ) -> GatewayRegistryResult<T>
    where
        T: FromRedisValue,
    {
        let (_permit, mut connection) = self.operation_connection().await?;
        query_script(&mut connection, source, keys, arguments)
            .await
            .map_err(|_| GatewayRegistryError::Unavailable)
    }

    async fn open_authenticated_connection(&self) -> GatewayRegistryResult<MultiplexedConnection> {
        let timeout = self.config.operation_timeout();
        let connection_config = AsyncConnectionConfig::new()
            .set_connection_timeout(Some(timeout))
            .set_response_timeout(Some(timeout));
        let mut connection = tokio::time::timeout(
            timeout,
            self.client
                .get_multiplexed_async_connection_with_config(&connection_config),
        )
        .await
        .map_err(|_| GatewayRegistryError::Unavailable)?
        .map_err(|_| GatewayRegistryError::Unavailable)?;
        tokio::time::timeout(
            timeout,
            redis::cmd("AUTH")
                .arg(self.credential.expose())
                .query_async::<()>(&mut connection),
        )
        .await
        .map_err(|_| GatewayRegistryError::Unavailable)?
        .map_err(|_| GatewayRegistryError::Unavailable)?;
        Ok(connection)
    }

    async fn verify_schema(
        &self,
        connection: &mut MultiplexedConnection,
    ) -> GatewayRegistryResult<()> {
        let status: i64 = query_script(
            connection,
            CHECK_SCHEMA,
            &[self.keys.schema()],
            &[GATEWAY_HA_SCHEMA_V2.to_string()],
        )
        .await
        .map_err(|_| GatewayRegistryError::Unavailable)?;
        status_result(status)
    }

    async fn operation_connection(
        &self,
    ) -> GatewayRegistryResult<(OwnedSemaphorePermit, MultiplexedConnection)> {
        let timeout = self.config.operation_timeout();
        let permit = tokio::time::timeout(timeout, self.slots.clone().acquire_owned())
            .await
            .map_err(|_| GatewayRegistryError::CapacityExceeded)?
            .map_err(|_| GatewayRegistryError::Unavailable)?;
        let connection = tokio::time::timeout(timeout, self.connection.read())
            .await
            .map_err(|_| GatewayRegistryError::Unavailable)?
            .clone()
            .ok_or(GatewayRegistryError::Unavailable)?;
        Ok((permit, connection))
    }
}

pub(crate) fn status_result(status: i64) -> GatewayRegistryResult<()> {
    match status {
        STATUS_OK => Ok(()),
        STATUS_CONFLICT => Err(GatewayRegistryError::Conflict),
        STATUS_STALE => Err(GatewayRegistryError::StaleOwner),
        STATUS_EXPIRED => Err(GatewayRegistryError::Expired),
        STATUS_UNSUPPORTED_SCHEMA => Err(GatewayRegistryError::UnsupportedSchema),
        STATUS_CAPACITY => Err(GatewayRegistryError::CapacityExceeded),
        STATUS_INVALID => Err(GatewayRegistryError::InvalidContract),
        _ => Err(GatewayRegistryError::Unavailable),
    }
}

async fn query_script<T>(
    connection: &mut MultiplexedConnection,
    source: &str,
    keys: &[String],
    arguments: &[String],
) -> redis::RedisResult<T>
where
    T: FromRedisValue,
{
    let mut command = redis::cmd("EVAL");
    command.arg(source).arg(keys.len());
    for key in keys {
        command.arg(key);
    }
    for argument in arguments {
        command.arg(argument);
    }
    command.query_async(connection).await
}

#[cfg(test)]
#[path = "redis_provider_tests.rs"]
mod tests;
