// =============================================================================
//        #######
//     ###       ###     F: redis_scripts.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Atomic single-slot scripts for Redis Gateway ownership and fencing.

pub(crate) const STATUS_OK: i64 = 1;
pub(crate) const STATUS_CONFLICT: i64 = -1;
pub(crate) const STATUS_STALE: i64 = -2;
pub(crate) const STATUS_EXPIRED: i64 = -3;
pub(crate) const STATUS_UNSUPPORTED_SCHEMA: i64 = -4;
pub(crate) const STATUS_CAPACITY: i64 = -5;
pub(crate) const STATUS_INVALID: i64 = -6;

pub(crate) const CHECK_SCHEMA: &str = r#"
local current = redis.call('GET', KEYS[1])
if not current then
  redis.call('SET', KEYS[1], ARGV[1])
  return 1
end
if current ~= ARGV[1] then return -4 end
return 1
"#;

pub(crate) const ACQUIRE_INSTANCE: &str = r#"
if redis.call('EXISTS', KEYS[2]) == 1 then
  if redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
  local current_expires = tonumber(redis.call('HGET', KEYS[2], 'expires'))
  if current_expires and current_expires > tonumber(ARGV[6]) then return -1 end
  redis.call('DEL', KEYS[2])
end
local epoch = redis.call('INCR', KEYS[1])
redis.call('HSET', KEYS[2],
  'schema', ARGV[1], 'tenant', ARGV[2], 'cluster', ARGV[3],
  'instance', ARGV[4], 'url', ARGV[5], 'epoch', epoch, 'expires', ARGV[7])
redis.call('PEXPIRE', KEYS[2], ARGV[8])
return epoch
"#;

pub(crate) const CHECK_INSTANCE: &str = r#"
local epoch = redis.call('HGET', KEYS[1], 'epoch')
if not epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if epoch ~= ARGV[1] then return -2 end
local expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
if not expires or expires <= tonumber(ARGV[2]) then
  redis.call('DEL', KEYS[1])
  return -3
end
return 1
"#;

pub(crate) const RENEW_INSTANCE: &str = r#"
local epoch = redis.call('HGET', KEYS[1], 'epoch')
if not epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if epoch ~= ARGV[1] then return -2 end
local expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
if not expires or expires <= tonumber(ARGV[2]) then
  redis.call('DEL', KEYS[1])
  return -3
end
redis.call('HSET', KEYS[1], 'expires', ARGV[3])
redis.call('PEXPIRE', KEYS[1], ARGV[4])
return 1
"#;

pub(crate) const RELEASE_INSTANCE: &str = r#"
local epoch = redis.call('HGET', KEYS[1], 'epoch')
if not epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if epoch ~= ARGV[1] then return -2 end
redis.call('DEL', KEYS[1])
return 1
"#;

pub(crate) const REGISTER_WORKER: &str = r#"
local lease_epoch = redis.call('HGET', KEYS[1], 'epoch')
if not lease_epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if lease_epoch ~= ARGV[1] then return -2 end
local lease_expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
if not lease_expires or lease_expires <= tonumber(ARGV[2]) then return -3 end
if redis.call('EXISTS', KEYS[2]) == 1
   and redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
local current_owner = redis.call('HGET', KEYS[2], 'owner_key')
local current_epoch = redis.call('HGET', KEYS[2], 'owner_epoch')
local current_expires = tonumber(redis.call('HGET', KEYS[2], 'expires'))
local current_generation = tonumber(redis.call('HGET', KEYS[2], 'generation'))
local current_live_epoch = current_owner and redis.call('HGET', current_owner, 'epoch') or nil
local current_lease_expires = current_owner
  and tonumber(redis.call('HGET', current_owner, 'expires')) or nil
if current_owner and current_expires and current_expires > tonumber(ARGV[2])
   and current_live_epoch == current_epoch and current_lease_expires
   and current_lease_expires > tonumber(ARGV[2])
   and (current_owner ~= KEYS[1] or current_epoch ~= ARGV[1]) then return -1 end
if current_owner == KEYS[1] and current_epoch == ARGV[1]
   and current_expires and current_expires > tonumber(ARGV[2])
   and current_generation and current_generation >= tonumber(ARGV[3]) then return -2 end
if redis.call('SCARD', KEYS[3]) > 64 then return -6 end
if redis.call('ZCARD', KEYS[4]) > tonumber(ARGV[7]) then return -6 end
redis.call('ZREMRANGEBYSCORE', KEYS[4], '-inf', ARGV[2])
if redis.call('EXISTS', KEYS[2]) == 0
   and redis.call('ZCARD', KEYS[4]) >= tonumber(ARGV[7]) then return -5 end
local old_caps = redis.call('SMEMBERS', KEYS[3])
for _, cap_key in ipairs(old_caps) do
  redis.call('ZREM', cap_key, KEYS[2])
  if redis.call('ZCARD', cap_key) == 0 then redis.call('DEL', cap_key) end
end
redis.call('DEL', KEYS[3])
redis.call('HSET', KEYS[2],
  'schema', 'appcore.gateway.ha.v2', 'owner_key', KEYS[1],
  'owner_epoch', ARGV[1], 'generation', ARGV[3],
  'expires', ARGV[4], 'record', ARGV[5])
redis.call('PEXPIRE', KEYS[2], ARGV[6])
redis.call('ZADD', KEYS[4], ARGV[4], KEYS[2])
redis.call('PEXPIRE', KEYS[4], 60000)
for index = 8, #ARGV do
  local cap_key = ARGV[index]
  redis.call('ZADD', cap_key, ARGV[4], KEYS[2])
  redis.call('PEXPIRE', cap_key, 60000)
  redis.call('SADD', KEYS[3], cap_key)
end
if #ARGV >= 8 then redis.call('PEXPIRE', KEYS[3], ARGV[6]) end
return 1
"#;

pub(crate) const RENEW_WORKER: &str = r#"
local lease_epoch = redis.call('HGET', KEYS[1], 'epoch')
if not lease_epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if lease_epoch ~= ARGV[1] then return -2 end
local lease_expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
if not lease_expires or lease_expires <= tonumber(ARGV[2]) then return -3 end
local owner_epoch = redis.call('HGET', KEYS[2], 'owner_epoch')
local owner_key = redis.call('HGET', KEYS[2], 'owner_key')
local generation = redis.call('HGET', KEYS[2], 'generation')
if not owner_epoch or not owner_key or not generation then return -3 end
if owner_key ~= KEYS[1] or owner_epoch ~= ARGV[1]
   or generation ~= ARGV[3] then return -2 end
if redis.call('SCARD', KEYS[3]) > 64 then return -6 end
if redis.call('ZCARD', KEYS[4]) > 1024 then return -6 end
redis.call('HSET', KEYS[2], 'expires', ARGV[4], 'record', ARGV[5])
redis.call('PEXPIRE', KEYS[2], ARGV[6])
local caps = redis.call('SMEMBERS', KEYS[3])
for _, cap_key in ipairs(caps) do
  redis.call('ZADD', cap_key, ARGV[4], KEYS[2])
  redis.call('PEXPIRE', cap_key, 60000)
end
if #caps > 0 then redis.call('PEXPIRE', KEYS[3], ARGV[6]) end
redis.call('ZADD', KEYS[4], ARGV[4], KEYS[2])
redis.call('PEXPIRE', KEYS[4], 60000)
return 1
"#;

pub(crate) const REMOVE_WORKER: &str = r#"
local lease_epoch = redis.call('HGET', KEYS[1], 'epoch')
if not lease_epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if lease_epoch ~= ARGV[1] then return -2 end
local owner_epoch = redis.call('HGET', KEYS[2], 'owner_epoch')
local owner_key = redis.call('HGET', KEYS[2], 'owner_key')
local generation = redis.call('HGET', KEYS[2], 'generation')
if not owner_epoch or not owner_key or not generation then return -3 end
if owner_key ~= KEYS[1] or owner_epoch ~= ARGV[1]
   or generation ~= ARGV[2] then return -2 end
if redis.call('SCARD', KEYS[3]) > 64 then return -6 end
if redis.call('ZCARD', KEYS[4]) > 1024 then return -6 end
local caps = redis.call('SMEMBERS', KEYS[3])
for _, cap_key in ipairs(caps) do
  redis.call('ZREM', cap_key, KEYS[2])
  if redis.call('ZCARD', cap_key) == 0 then redis.call('DEL', cap_key) end
end
redis.call('ZREM', KEYS[4], KEYS[2])
redis.call('DEL', KEYS[2], KEYS[3])
return 1
"#;

pub(crate) const RESOLVE_WORKER: &str = r#"
local record = redis.call('HGET', KEYS[1], 'record')
if not record then return nil end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return '!' end
local expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
if not expires or expires <= tonumber(ARGV[1]) then return nil end
local owner_key = redis.call('HGET', KEYS[1], 'owner_key')
local owner_epoch = redis.call('HGET', KEYS[1], 'owner_epoch')
if not owner_key or not owner_epoch then return nil end
local live_epoch = redis.call('HGET', owner_key, 'epoch')
local lease_expires = tonumber(redis.call('HGET', owner_key, 'expires'))
if redis.call('HGET', owner_key, 'schema') ~= 'appcore.gateway.ha.v2' then return '!' end
if not live_epoch or live_epoch ~= owner_epoch or not lease_expires
   or lease_expires <= tonumber(ARGV[1]) then return nil end
return record
"#;

pub(crate) const RESOLVE_CAPABILITY: &str = r#"
if redis.call('ZCARD', KEYS[1]) > 1024 then return {'!'} end
redis.call('ZREMRANGEBYSCORE', KEYS[1], '-inf', ARGV[1])
local workers = redis.call('ZRANGEBYSCORE', KEYS[1], '(' .. ARGV[1], '+inf',
  'LIMIT', 0, ARGV[2])
local records = {}
for _, worker_key in ipairs(workers) do
  local record = redis.call('HGET', worker_key, 'record')
  if redis.call('HGET', worker_key, 'schema') ~= 'appcore.gateway.ha.v2' then return {'!'} end
  local expires = tonumber(redis.call('HGET', worker_key, 'expires'))
  local owner_key = redis.call('HGET', worker_key, 'owner_key')
  local owner_epoch = redis.call('HGET', worker_key, 'owner_epoch')
  local live_epoch = owner_key and redis.call('HGET', owner_key, 'epoch') or nil
  local lease_expires = owner_key and tonumber(redis.call('HGET', owner_key, 'expires')) or nil
  if owner_key and redis.call('HGET', owner_key, 'schema') ~= 'appcore.gateway.ha.v2'
     then return {'!'} end
  if record and expires and expires > tonumber(ARGV[1]) and live_epoch
     and live_epoch == owner_epoch and lease_expires and lease_expires > tonumber(ARGV[1]) then
    table.insert(records, record)
  else
    redis.call('ZREM', KEYS[1], worker_key)
  end
end
return records
"#;

pub(crate) const REGISTER_SESSION: &str = r#"
local lease_epoch = redis.call('HGET', KEYS[1], 'epoch')
if not lease_epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if lease_epoch ~= ARGV[1] then return -2 end
local lease_expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
if not lease_expires or lease_expires <= tonumber(ARGV[2]) then return -3 end
if redis.call('EXISTS', KEYS[2]) == 1
   and redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if redis.call('ZCARD', KEYS[3]) > tonumber(ARGV[6]) then return -6 end
redis.call('ZREMRANGEBYSCORE', KEYS[3], '-inf', ARGV[2])
if redis.call('EXISTS', KEYS[2]) == 0
   and redis.call('ZCARD', KEYS[3]) >= tonumber(ARGV[6]) then return -5 end
redis.call('HSET', KEYS[2], 'schema', 'appcore.gateway.ha.v2',
  'owner_key', KEYS[1], 'owner_epoch', ARGV[1],
  'expires', ARGV[3], 'record', ARGV[4])
redis.call('PEXPIRE', KEYS[2], ARGV[5])
redis.call('ZADD', KEYS[3], ARGV[3], KEYS[2])
redis.call('PEXPIRE', KEYS[3], 60000)
return 1
"#;

pub(crate) const REMOVE_SESSION: &str = r#"
local owner_epoch = redis.call('HGET', KEYS[1], 'owner_epoch')
local owner_key = redis.call('HGET', KEYS[1], 'owner_key')
if not owner_epoch or not owner_key then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if owner_key ~= KEYS[2] or owner_epoch ~= ARGV[1] then return -2 end
redis.call('ZREM', KEYS[3], KEYS[1])
redis.call('DEL', KEYS[1])
return 1
"#;

pub(crate) const CLAIM_REQUEST: &str = r#"
if redis.call('EXISTS', KEYS[4]) == 1 then
  if redis.call('HGET', KEYS[4], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
  local current_expires = tonumber(redis.call('HGET', KEYS[4], 'expires'))
  if current_expires and current_expires > tonumber(ARGV[4]) then return -1 end
  redis.call('DEL', KEYS[4])
end
local origin_epoch = redis.call('HGET', KEYS[1], 'epoch')
local target_epoch = redis.call('HGET', KEYS[2], 'epoch')
if not origin_epoch or not target_epoch then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[3], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if origin_epoch ~= ARGV[1] or target_epoch ~= ARGV[2] then return -2 end
local origin_expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
local target_expires = tonumber(redis.call('HGET', KEYS[2], 'expires'))
if not origin_expires or not target_expires or origin_expires <= tonumber(ARGV[4])
   or target_expires <= tonumber(ARGV[4]) then return -3 end
local worker_epoch = redis.call('HGET', KEYS[3], 'owner_epoch')
local worker_owner_key = redis.call('HGET', KEYS[3], 'owner_key')
local generation = redis.call('HGET', KEYS[3], 'generation')
local worker_expires = tonumber(redis.call('HGET', KEYS[3], 'expires'))
if not worker_epoch or not worker_owner_key or not generation
   or not worker_expires then return -3 end
if worker_owner_key ~= KEYS[2] or worker_epoch ~= ARGV[2]
   or generation ~= ARGV[3] then return -2 end
if worker_expires <= tonumber(ARGV[4]) then return -3 end
if redis.call('ZCARD', KEYS[5]) > tonumber(ARGV[8]) then return -6 end
redis.call('ZREMRANGEBYSCORE', KEYS[5], '-inf', ARGV[4])
if redis.call('ZCARD', KEYS[5]) >= tonumber(ARGV[8]) then return -5 end
redis.call('HSET', KEYS[4], 'schema', 'appcore.gateway.ha.v2',
  'origin_key', KEYS[1], 'origin_epoch', ARGV[1],
  'target_key', KEYS[2], 'target_epoch', ARGV[2], 'worker_key', KEYS[3],
  'generation', ARGV[3], 'expires', ARGV[5], 'record', ARGV[6])
redis.call('PEXPIRE', KEYS[4], ARGV[7])
redis.call('ZADD', KEYS[5], ARGV[5], KEYS[4])
redis.call('PEXPIRE', KEYS[5], 30000)
return 1
"#;

pub(crate) const COMPLETE_REQUEST: &str = r#"
local origin_epoch = redis.call('HGET', KEYS[4], 'origin_epoch')
local target_epoch = redis.call('HGET', KEYS[4], 'target_epoch')
local generation = redis.call('HGET', KEYS[4], 'generation')
local origin_key = redis.call('HGET', KEYS[4], 'origin_key')
local target_key = redis.call('HGET', KEYS[4], 'target_key')
local worker_key = redis.call('HGET', KEYS[4], 'worker_key')
if not origin_epoch or not target_epoch or not generation
   or not origin_key or not target_key or not worker_key then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[3], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[4], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if origin_key ~= KEYS[1] or target_key ~= KEYS[2]
   or worker_key ~= KEYS[3] then return -2 end
if origin_epoch ~= ARGV[1] or target_epoch ~= ARGV[2]
   or generation ~= ARGV[3] then return -2 end
local request_expires = tonumber(redis.call('HGET', KEYS[4], 'expires'))
if not request_expires or request_expires <= tonumber(ARGV[4]) then return -3 end
local live_origin = redis.call('HGET', KEYS[1], 'epoch')
local live_target = redis.call('HGET', KEYS[2], 'epoch')
local live_worker_epoch = redis.call('HGET', KEYS[3], 'owner_epoch')
local live_worker_owner = redis.call('HGET', KEYS[3], 'owner_key')
local live_generation = redis.call('HGET', KEYS[3], 'generation')
if live_origin ~= ARGV[1] or live_target ~= ARGV[2]
   or live_worker_owner ~= KEYS[2] or live_worker_epoch ~= ARGV[2]
   or live_generation ~= ARGV[3] then return -2 end
local origin_expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
local target_expires = tonumber(redis.call('HGET', KEYS[2], 'expires'))
local worker_expires = tonumber(redis.call('HGET', KEYS[3], 'expires'))
if not origin_expires or not target_expires or not worker_expires
   or origin_expires <= tonumber(ARGV[4]) or target_expires <= tonumber(ARGV[4])
   or worker_expires <= tonumber(ARGV[4]) then return -3 end
redis.call('DEL', KEYS[4])
redis.call('ZREM', KEYS[5], KEYS[4])
return 1
"#;

pub(crate) const CHECK_REQUEST: &str = r#"
local origin_epoch = redis.call('HGET', KEYS[4], 'origin_epoch')
local target_epoch = redis.call('HGET', KEYS[4], 'target_epoch')
local generation = redis.call('HGET', KEYS[4], 'generation')
local origin_key = redis.call('HGET', KEYS[4], 'origin_key')
local target_key = redis.call('HGET', KEYS[4], 'target_key')
local worker_key = redis.call('HGET', KEYS[4], 'worker_key')
local record = redis.call('HGET', KEYS[4], 'record')
if not origin_epoch or not target_epoch or not generation
   or not origin_key or not target_key or not worker_key or not record then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[2], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[3], 'schema') ~= 'appcore.gateway.ha.v2'
   or redis.call('HGET', KEYS[4], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if origin_key ~= KEYS[1] or target_key ~= KEYS[2] or worker_key ~= KEYS[3]
   or origin_epoch ~= ARGV[1] or target_epoch ~= ARGV[2]
   or generation ~= ARGV[3] or record ~= ARGV[5] then return -2 end
local request_expires = tonumber(redis.call('HGET', KEYS[4], 'expires'))
if not request_expires or request_expires <= tonumber(ARGV[4]) then return -3 end
local live_origin = redis.call('HGET', KEYS[1], 'epoch')
local live_target = redis.call('HGET', KEYS[2], 'epoch')
local live_worker_epoch = redis.call('HGET', KEYS[3], 'owner_epoch')
local live_worker_owner = redis.call('HGET', KEYS[3], 'owner_key')
local live_generation = redis.call('HGET', KEYS[3], 'generation')
if live_origin ~= ARGV[1] or live_target ~= ARGV[2]
   or live_worker_owner ~= KEYS[2] or live_worker_epoch ~= ARGV[2]
   or live_generation ~= ARGV[3] then return -2 end
local origin_expires = tonumber(redis.call('HGET', KEYS[1], 'expires'))
local target_expires = tonumber(redis.call('HGET', KEYS[2], 'expires'))
local worker_expires = tonumber(redis.call('HGET', KEYS[3], 'expires'))
if not origin_expires or not target_expires or not worker_expires
   or origin_expires <= tonumber(ARGV[4]) or target_expires <= tonumber(ARGV[4])
   or worker_expires <= tonumber(ARGV[4]) then return -3 end
return 1
"#;

pub(crate) const CANCEL_REQUEST: &str = r#"
local origin_epoch = redis.call('HGET', KEYS[1], 'origin_epoch')
local origin_key = redis.call('HGET', KEYS[1], 'origin_key')
if not origin_epoch or not origin_key then return -3 end
if redis.call('HGET', KEYS[1], 'schema') ~= 'appcore.gateway.ha.v2' then return -4 end
if origin_key ~= KEYS[2] or origin_epoch ~= ARGV[1] then return -2 end
redis.call('ZREM', KEYS[3], KEYS[1])
redis.call('DEL', KEYS[1])
return 1
"#;
