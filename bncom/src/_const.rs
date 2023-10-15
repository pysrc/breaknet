// pub const BUFF_SIZE: usize = 2048;
// SESSION_MAX 最大会话数量
pub const SESSION_MAX: usize = 1<<16;

// SESSION_CAP 掩码,必须字节全1，例如0b1 0b11 0b111 0b1111...
pub const SESSION_CAP_MASK: usize = 0b111;
// 每个session待连接数组
pub const SESSION_CAP: usize = SESSION_CAP_MASK + 1;

// 心跳时间s
pub const HEARTBEAT_TIME: u64 = 3;
// 心跳超时时间
pub const HEARTBEAT_TIMEOUT: u64 = 6;

// START 第一次连接服务器
pub const START: u8 = 1;
// NEWSOCKET 新连接
pub const NEWSOCKET: u8 = 2;
// NEWCONN 新连接发送到服务端命令
pub const NEWCONN: u8 = 3;
// ERROR 处理失败
pub const ERROR: u8 = 4;
// SUCCESS 处理成功
pub const SUCCESS: u8 = 5;
// IDLE 空闲命令 什么也不做
pub const IDLE: u8 = 6;
// KILL 退出命令
pub const KILL: u8 = 7;
// ERROR_PWD 密码错误
pub const ERROR_PWD: u8 = 8;
// ERROR_BUSY 端口被占用
pub const ERROR_BUSY: u8 = 9;
// ERROR_LIMIT_PORT 不满足端口范围
pub const ERROR_LIMIT_PORT: u8 = 10;
// ERROR_SESSION_OVER session满了
pub const ERROR_SESSION_OVER: u8 = 11;

