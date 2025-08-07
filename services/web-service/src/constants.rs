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
