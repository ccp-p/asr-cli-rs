use thiserror::Error;

#[derive(Error, Debug)]
pub enum AudioProcessorError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("依赖检查失败: {0}")]
    DependencyCheckFailed(String),
    
    #[error("外部程序未找到: {0}")]
    ExternalProgramNotFound(String),
    
    #[error("处理错误: {0}")]
    ProcessingError(String),
    
    #[error("HTTP请求错误: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("ASR服务错误: {0}")]
    ASRServiceError(String),
    
    #[error("用户中断")]
    Interrupted,
}

pub type Result<T> = std::result::Result<T, AudioProcessorError>;