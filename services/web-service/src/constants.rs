pub const BALLOT_CODE_RANDOM_LENGTH: usize = 8;

pub const LUA_SCRIPT_GET_FINAL_ORDER: &str = r#"
local fields = ARGV

-- 获取 op_stats 哈希表的所有字段值
local stats = redis.call('HMGET', 'op_stats', unpack(fields))

-- 获取 total_valid_ballots
local total_ballots = redis.call('GET', 'total_valid_ballots')

-- 返回两个值：stats 和 total_ballots
return {stats, total_ballots}
"#;
