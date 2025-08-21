pub const BALLOT_CODE_RANDOM_LENGTH: usize = 8;

pub const LUA_SCRIPT_GET_FINAL_ORDER: &str = r#"
local topic_id = KEYS[1]
local fields = ARGV

local stats_key = topic_id .. ':op_stats'
local stats = redis.call('HMGET', stats_key, unpack(fields))

local valid_ballots_key = topic_id .. ':valid_ballots_count'
local total_ballots = redis.call('GET', valid_ballots_key)

return {stats, total_ballots}
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
-- ARGV: topic_id1, win_id1, lose_id1, multiplier1, topic_id2, win_id2, lose_id2, multiplier2, ...
-- Each score update takes 4 arguments: topic_id, win_id, lose_id, multiplier

-- local valid_ballots_count = KEYS[1]
local arg_count = #ARGV

-- 确保参数数量是4的倍数
if arg_count % 4 ~= 0 then
    return redis.error_reply("invalid argument count: must be multiple of 4")
end

for i = 1, arg_count, 4 do
    local topic_id = ARGV[i]
    local win_id = tonumber(ARGV[i + 1])
    local lose_id = tonumber(ARGV[i + 2])
    local multiplier = tonumber(ARGV[i + 3])

    local op_stats_key = topic_id .. ":op_stats"
    local op_matrix_key = topic_id .. ":op_matrix"

    redis.call("HINCRBY", op_stats_key, win_id..":win", multiplier)
    redis.call("HINCRBY", op_stats_key, lose_id..":lose", multiplier)
    redis.call("HINCRBY", op_matrix_key, win_id..":"..lose_id, multiplier)
    redis.call("HINCRBY", op_matrix_key, lose_id..":"..win_id, -multiplier)

    local valid_ballots_key = topic_id .. ":valid_ballots_count"
    redis.call("INCR", valid_ballots_key)
end

return 1
"#;
