use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use anyhow::Result;
use log::{error, warn, info};
use std::time::Duration;
use std::thread;

/// 音频工具错误类型
#[derive(Debug)]
pub enum AudioToolsError {
    /// IO错误
    IoError(std::io::Error),
    /// 配置错误
    ConfigError(String),
    /// ASR服务错误
    AsrServiceError(String),
    /// 文件处理错误
    FileProcessingError(String),
    /// 认证错误
    AuthenticationError(String),
    /// 网络错误
    NetworkError(String),
    /// 通用错误
    General(String),
}

impl fmt::Display for AudioToolsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioToolsError::IoError(err) => write!(f, "IO错误: {}", err),
            AudioToolsError::ConfigError(msg) => write!(f, "配置错误: {}", msg),
            AudioToolsError::AsrServiceError(msg) => write!(f, "ASR服务错误: {}", msg),
            AudioToolsError::FileProcessingError(msg) => write!(f, "文件处理错误: {}", msg),
            AudioToolsError::AuthenticationError(msg) => write!(f, "认证错误: {}", msg),
            AudioToolsError::NetworkError(msg) => write!(f, "网络错误: {}", msg),
            AudioToolsError::General(msg) => write!(f, "错误: {}", msg),
        }
    }
}

impl std::error::Error for AudioToolsError {}

impl From<std::io::Error> for AudioToolsError {
    fn from(err: std::io::Error) -> Self {
        AudioToolsError::IoError(err)
    }
}

impl From<&str> for AudioToolsError {
    fn from(err: &str) -> Self {
        AudioToolsError::General(err.to_string())
    }
}

impl From<String> for AudioToolsError {
    fn from(err: String) -> Self {
        AudioToolsError::General(err)
    }
}

impl From<anyhow::Error> for AudioToolsError {
    fn from(err: anyhow::Error) -> Self {
        AudioToolsError::General(err.to_string())
    }
}

/// 错误计数结构
struct ErrorCounter {
    error_counts: HashMap<String, usize>,
    total_retries: usize,
    total_failures: usize,
    total_successes: usize,
}

/// 错误处理器
pub struct ErrorHandler {
    max_retries: u32,
    retry_delay: f64,
    counters: Mutex<ErrorCounter>,
}

impl ErrorHandler {
    /// 创建新的错误处理器
    pub fn new(max_retries: u32, retry_delay: f64) -> Self {
        Self {
            max_retries,
            retry_delay,
            counters: Mutex::new(ErrorCounter {
                error_counts: HashMap::new(),
                total_retries: 0,
                total_failures: 0,
                total_successes: 0,
            }),
        }
    }
    
    /// 安全执行函数，自动处理重试逻辑
    pub fn safe_execute<F, T>(&self, f: F, error_context: &str) -> Result<T>
    where
        F: Fn() -> Result<T>,
    {
        let mut retry_count = 0;
        let mut last_error = None;
        
        loop {
            match f() {
                Ok(result) => {
                    // 计数成功
                    {
                        let mut counters = self.counters.lock().unwrap();
                        counters.total_successes += 1;
                    }
                    return Ok(result);
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    
                    // 更新错误计数
                    {
                        let mut counters = self.counters.lock().unwrap();
                        let entry = counters.error_counts.entry(error_context.to_string()).or_insert(0);
                        *entry += 1;
                        counters.total_retries += 1;
                    }
                    
                    retry_count += 1;
                    if retry_count <= self.max_retries {
                        let delay = Duration::from_secs_f64(self.retry_delay * (retry_count as f64));
                        warn!("{} - 重试 {}/{}, 延迟 {:.1}秒: {}", 
                            error_context, retry_count, self.max_retries, delay.as_secs_f64(), 
                            last_error.as_ref().unwrap());
                        
                        thread::sleep(delay);
                        continue;
                    } else {
                        // 计数失败
                        {
                            let mut counters = self.counters.lock().unwrap();
                            counters.total_failures += 1;
                        }
                        
                        error!("{} - 重试 {}/{} 次后失败: {}", 
                            error_context, retry_count - 1, self.max_retries, 
                            last_error.as_ref().unwrap());
                        return Err(anyhow::anyhow!("{}: {}", error_context, last_error.unwrap()));
                    }
                }
            }
        }
    }
    
    /// 打印错误统计信息
    pub fn print_error_stats(&self) {
        let counters = self.counters.lock().unwrap();
        
        if counters.total_failures > 0 || counters.total_retries > 0 {
            info!("\n错误统计:");
            info!("总计重试次数: {}", counters.total_retries);
            info!("总计失败次数: {}", counters.total_failures);
            info!("总计成功次数: {}", counters.total_successes);
            
            if !counters.error_counts.is_empty() {
                info!("错误类型分布:");
                for (error_type, count) in &counters.error_counts {
                    info!("  {}: {} 次", error_type, count);
                }
            }
        }
    }
}