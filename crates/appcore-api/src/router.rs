// =============================================================================
//        #######
//     ###       ###     F: router.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! In-memory API router contracts.

use std::collections::HashMap;
use std::sync::Arc;

use appcore_core::{RuntimeError, RuntimeResult};

use crate::api::{ApiRequest, ApiResponse};
use crate::command_endpoint::CommandEndpoint;
use crate::query_endpoint::{QueryEndpoint, QueryName};

/// Minimal cloneable router for one command endpoint and multiple query endpoints.
///
/// Clones share an immutable endpoint snapshot. Runtime hosts freeze query
/// registration after bootstrap and clone the router before dispatch so no
/// host-state lock is retained while an endpoint executes.
#[derive(Clone, Default)]
pub struct ApiRouter {
    state: Arc<ApiRouterState>,
}

#[derive(Clone, Default)]
struct ApiRouterState {
    command_endpoint: Option<Arc<dyn CommandEndpoint>>,
    queries: HashMap<QueryName, Arc<dyn QueryEndpoint>>,
    queries_frozen: bool,
}

impl ApiRouter {
    /// Creates an empty transport-neutral router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the command endpoint used by this router.
    pub fn set_command_endpoint<E: CommandEndpoint + 'static>(&mut self, endpoint: E) {
        Arc::make_mut(&mut self.state).command_endpoint = Some(Arc::new(endpoint));
    }

    /// Registers one uniquely named application query endpoint.
    ///
    /// Registration fails after [`Self::freeze_queries`].
    pub fn register_query<E: QueryEndpoint + 'static>(&mut self, endpoint: E) -> RuntimeResult<()> {
        let name = endpoint.query_name().clone();
        if self.state.queries_frozen {
            return Err(RuntimeError::InvalidRequest {
                kind: "query",
                reason: "router_frozen",
            });
        }
        if self.state.queries.contains_key(&name) {
            return Err(RuntimeError::RegistryItemAlreadyRegistered {
                kind: "query",
                name: name.as_str().to_string(),
            });
        }
        Arc::make_mut(&mut self.state)
            .queries
            .insert(name, Arc::new(endpoint));
        Ok(())
    }

    /// Freezes application query registration while preserving dispatch.
    pub fn freeze_queries(&mut self) {
        Arc::make_mut(&mut self.state).queries_frozen = true;
    }

    /// Reports whether application query registration is frozen.
    pub fn queries_are_frozen(&self) -> bool {
        self.state.queries_frozen
    }

    /// Reports whether a query endpoint is registered.
    pub fn has_query(&self, name: &QueryName) -> bool {
        self.state.queries.contains_key(name)
    }

    /// Returns registered query names in deterministic lexical order.
    pub fn query_names(&self) -> Vec<QueryName> {
        let mut names = self.state.queries.keys().cloned().collect::<Vec<_>>();
        names.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        names
    }

    /// Dispatches a request to a named query endpoint.
    pub fn dispatch_query(
        &self,
        name: &QueryName,
        request: ApiRequest,
    ) -> RuntimeResult<ApiResponse> {
        let Some(endpoint) = self.state.queries.get(name) else {
            return Err(RuntimeError::RegistryItemNotFound {
                kind: "query",
                name: name.as_str().to_string(),
            });
        };
        endpoint.handle_query(request)
    }

    /// Dispatches a request to the configured command endpoint.
    pub fn dispatch_command(&self, request: ApiRequest) -> RuntimeResult<ApiResponse> {
        let Some(endpoint) = &self.state.command_endpoint else {
            return Err(RuntimeError::MissingConfiguration {
                name: "command_endpoint",
            });
        };
        endpoint.handle_command(request)
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
