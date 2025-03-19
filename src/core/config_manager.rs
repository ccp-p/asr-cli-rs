use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use log::{info, warn, error};
use serde_json::Value;
use thiserror::Error;

/// 配置验证错误
#[derive(Error, Debug)]
pub enum ConfigValidationError {
    #[error("缺少必需的配置项: {0}")]
    MissingRequiredField(String),
    
    #[error("配置项类型错误 {0}: 期望 {1}, 实际为 {2}")]
    TypeMismatch(String, String, String),
    
    #[error("无效的路径: {0}")]
    InvalidPath(String),
    
    #[error("文件不存在: {0}")]
    FileNotExists(String),
    
    #[error("无法读取配置文件: {0}")]
    ReadError(#[from] std::io::Error),
    
    #[error("无法解析JSON: {0}")]
    ParseError(#[from] serde_json::Error),
    
    #[error("通用错误: {0}")]
    GeneralError(String),
}

/// 配置管理器
pub struct ConfigManager {
    config: HashMap<String, Value>,
    config_file: Option<PathBuf>,
}

impl ConfigManager {
    /// 创建新的配置管理器
    pub fn new(config_file: Option<&Path>) -> Result<Self> {
        let mut config_manager = Self {
            config: HashMap::new(),
            config_file: config_file.map(PathBuf::from),
        };
        
        // 设置默认配置
        config_manager.set_defaults();
        
        // 如果提供了配置文件，从文件加载配置
        if let Some(path) = config_file {
            if path.exists() {
                config_manager.load_from_file(path)
                    .with_context(|| format!("从文件加载配置失败: {}", path.display()))?;
            } else {
                warn!("配置文件不存在, 将使用默认值: {}", path.display());
            }
        }
        
        Ok(config_manager)
    }
    
    /// 设置默认配置
    fn set_defaults(&mut self) {
        self.config.insert("media_folder".to_string(), Value::String("D:/download/".to_string()));
        self.config.insert("output_folder".to_string(), Value::String("D:/download/dest/".to_string()));
        self.config.insert("max_retries".to_string(), Value::Number(3.into()));
        self.config.insert("max_workers".to_string(), Value::Number(4.into()));
        self.config.insert("use_jianying_first".to_string(), Value::Bool(false));
        self.config.insert("use_kuaishou".to_string(), Value::Bool(false));
        self.config.insert("use_bcut".to_string(), Value::Bool(true));
        self.config.insert("format_text".to_string(), Value::Bool(true));
        self.config.insert("include_timestamps".to_string(), Value::Bool(true));
        self.config.insert("show_progress".to_string(), Value::Bool(true));
        self.config.insert("process_video".to_string(), Value::Bool(true));
        self.config.insert("extract_audio_only".to_string(), Value::Bool(false));
        self.config.insert("watch_mode".to_string(), Value::Bool(false));
        self.config.insert("max_part_time".to_string(), Value::Number(30.into()));
        self.config.insert("retry_delay".to_string(), Value::Number(1.5.into()));
    }
    
    /// 从文件加载配置
    pub fn load_from_file(&mut self, path: &Path) -> Result<()> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        let loaded_config: HashMap<String, Value> = serde_json::from_str(&contents)?;
        
        // 更新现有配置
        for (key, value) in loaded_config {
            self.config.insert(key, value);
        }
        
        info!("从文件加载了配置: {}", path.display());
        
        Ok(())
    }
    
    /// 保存配置到文件
    pub fn save_config(&self, path: &Path) -> Result<()> {
        let config_json = serde_json::to_string_pretty(&self.config)?;
        
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let mut file = File::create(path)?;
        file.write_all(config_json.as_bytes())?;
        
        info!("配置已保存到: {}", path.display());
        
        Ok(())
    }
    
    /// 更新配置
    pub fn update(&mut self, updates: &HashMap<String, Value>) -> Result<()> {
        for (key, value) in updates {
            self.config.insert(key.clone(), value.clone());
        }
        
        // 如果有配置文件，保存更新后的配置
        if let Some(ref path) = self.config_file {
            self.save_config(path)?;
        }
        
        Ok(())
    }
    
    /// 获取配置值
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.config.get(key)
    }
    
    /// 获取配置字典副本
    pub fn as_dict(&self) -> HashMap<String, Value> {
        self.config.clone()
    }
    
    /// 打印当前配置
    pub fn print_config(&self) {
        info!("当前配置:");
        for (key, value) in &self.config {
            info!("  {}: {}", key, value);
        }
    }
    
    /// 验证配置项
    pub fn validate_config(&self) -> std::result::Result<(), ConfigValidationError> {
        // 验证必需的字段
        let required_fields = vec!["media_folder", "output_folder"];
        for field in required_fields {
            if !self.config.contains_key(field) {
                return Err(ConfigValidationError::MissingRequiredField(field.to_string()));
            }
        }
        
        // 验证文件夹路径
        if let Some(Value::String(path)) = self.config.get("media_folder") {
            let media_path = Path::new(path);
            if !media_path.exists() {
                return Err(ConfigValidationError::FileNotExists(format!("媒体文件夹不存在: {}", path)));
            }
            if !media_path.is_dir() {
                return Err(ConfigValidationError::InvalidPath(format!("媒体路径不是目录: {}", path)));
            }
        }
        
        // 验证输出文件夹
        if let Some(Value::String(path)) = self.config.get("output_folder") {
            let output_path = Path::new(path);
            if output_path.exists() && !output_path.is_dir() {
                return Err(ConfigValidationError::InvalidPath(format!("输出路径不是目录: {}", path)));
            }
        }
        
        Ok(())
    }
}