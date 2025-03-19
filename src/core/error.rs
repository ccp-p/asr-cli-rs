use thiserror::Error;

#[derive(Error, Debug)]
pub enum AudioProcessorError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("ASR error: {0}")]
    AsrError(String),
}