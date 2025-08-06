use std::time::Duration;

pub const CONSUMER_BATCH_SIZE: usize = 100;
pub const CONSUMER_RETRY_DELAY: Duration = Duration::from_secs(5);
pub const DLQ_RETRY_DELAY: Duration = Duration::from_secs(10);
pub const DLQ_MAX_RETRIES: u32 = 5;

pub const LUA_SCRIPT_UPDATE_SCORES: &str = r#"
local win_id = ARGV[1]
local lose_id = ARGV[2]
local multiplier = ARGV[3]

redis.call("HINCRBY", "op_stats", win_id..":win", multiplier)
redis.call("HINCRBY", "op_stats", lose_id..":lose", multiplier)
redis.call("HINCRBY", "op_matrix", win_id..":"..lose_id, multiplier)
redis.call("HINCRBY", "op_matrix", lose_id..":"..win_id, -multiplier)

redis.call("INCR", "total_valid_ballots")

return 1
"#;

pub const LUA_SCRIPT_IP_COUNTER: &str = r#"
local counter_key = KEYS[1]
local expire_seconds = ARGV[1]
local max_ip_limit = tonumber(ARGV[2])
local base_multiplier = tonumber(ARGV[3])
local low_multiplier = tonumber(ARGV[4])

local current = redis.call("INCR", counter_key)
redis.call("EXPIRE", counter_key, expire_seconds)

local multiplier
if max_ip_limit < 0 or current <= max_ip_limit then
    multiplier = base_multiplier
else
    multiplier = low_multiplier
end

return multiplier
"#;

pub const LUA_SCRIPT_BATCH_IP_COUNTER_SCRIPT: &str = r#"
local expire_seconds = ARGV[1]
local max_ip_limit = tonumber(ARGV[2])
local base_multiplier = tonumber(ARGV[3])
local low_multiplier = tonumber(ARGV[4])

local results = {}

for i = 1, #KEYS do
    local key = KEYS[i]
    
    local current = redis.call('INCR', key)
    redis.call('EXPIRE', key, expire_seconds)
    
    local multiplier
    if max_ip_limit < 0 or current <= max_ip_limit then
        multiplier = base_multiplier
    else
        multiplier = low_multiplier
    end
    
    results[i] = multiplier
end

return results
"#;

pub const LUA_SCRIPT_BATCH_SCORE_UPDATE_SCRIPT: &str = r#"
-- KEYS: empty (we use ARGV for dynamic key generation)
-- ARGV: win_id1, lose_id1, multiplier1, win_id2, lose_id2, multiplier2, ...
-- Each score update takes 3 arguments: win_id, lose_id, multiplier

local valid_ballots_count = KEYS[1]
local arg_count = #ARGV

-- 确保参数数量是3的倍数
if arg_count % 3 ~= 0 then
    return redis.error_reply("invalid argument count: must be multiple of 3")
end

for i = 1, arg_count, 3 do
    local win_id = ARGV[i]
    local lose_id = ARGV[i + 1]
    local multiplier = ARGV[i + 2]

    redis.call("HINCRBY", "op_stats", win_id..":win", multiplier)
    redis.call("HINCRBY", "op_stats", lose_id..":lose", multiplier)
    redis.call("HINCRBY", "op_matrix", win_id..":"..lose_id, multiplier)
    redis.call("HINCRBY", "op_matrix", lose_id..":"..win_id, -multiplier)
end

redis.call("INCRBY", "total_valid_ballots", valid_ballots_count)

return 1
"#;

pub const LUA_SCRIPT_GET_DEL_MANY: &str = r#"
local results = {}
for i, key in ipairs(KEYS) do
    local value = redis.call("GET", key)
    if value then
        redis.call("DEL", key)
    end
    results[i] = value
end
return results
"#;

pub const LUA_SCRIPT_DEL_MUTIPLE: &str = r#"
for i, key in ipairs(KEYS) do
    redis.call("DEL", key)
end
return 1
"#;
