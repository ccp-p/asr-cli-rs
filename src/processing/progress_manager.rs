use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use log::debug;

/// 进度管理器，用于创建和管理进度条
pub struct ProgressManager {
    /// 多进度条管理器
    multi_progress: Arc<MultiProgress>,
    
    /// 进度条映射
    progress_bars: Mutex<HashMap<String, ProgressBar>>,
    
    /// 是否显示进度
    show_progress: bool,
}

impl ProgressManager {
    /// 创建新的进度管理器
    pub fn new(show_progress: bool) -> Self {
        Self {
            multi_progress: Arc::new(MultiProgress::new()),
            progress_bars: Mutex::new(HashMap::new()),
            show_progress,
        }
    }
    
    /// 创建进度条
    pub fn create_progress_bar(&self, name: &str, total: usize, prefix: &str, description: Option<&str>) -> Option<ProgressBar> {
        if !self.show_progress {
            return None;
        }
        
        let mut bars = self.progress_bars.lock().unwrap();
        
        // 如果已经存在，先删除旧的
        if bars.contains_key(name) {
            self.finish_progress_internal(&mut bars, name, None);
        }
        
        let progress_bar = self.multi_progress.add(ProgressBar::new(total as u64));
        
        // 设置样式
        progress_bar.set_style(
            ProgressStyle::with_template(
                "{prefix:.bold.dim} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}"
            )
            .unwrap()
            .progress_chars("█▓▒░  ")
        );
        
        // 设置前缀
        progress_bar.set_prefix(prefix.to_string());
        
        // 设置初始描述
        if let Some(desc) = description {
            progress_bar.set_message(desc.to_string());
        }
        
        // 存储进度条
        bars.insert(name.to_string(), progress_bar.clone());
        
        debug!("创建进度条: {}", name);
        
        Some(progress_bar)
    }
    
    /// 更新进度
    pub fn update_progress(&self, name: &str, position: usize, message: Option<&str>) {
        if !self.show_progress {
            return;
        }
        
        let bars = self.progress_bars.lock().unwrap();
        if let Some(bar) = bars.get(name) {
            bar.set_position(position as u64);
            
            if let Some(msg) = message {
                bar.set_message(msg.to_string());
            }
        }
    }
    
    /// 增加进度
    pub fn increment_progress(&self, name: &str, amount: usize, message: Option<&str>) {
        if !self.show_progress {
            return;
        }
        
        let bars = self.progress_bars.lock().unwrap();
        if let Some(bar) = bars.get(name) {
            bar.inc(amount as u64);
            
            if let Some(msg) = message {
                bar.set_message(msg.to_string());
            }
        }
    }
    
    /// 完成进度条
    pub fn finish_progress(&self, name: &str, message: Option<&str>) {
        if !self.show_progress {
            return;
        }
        
        let mut bars = self.progress_bars.lock().unwrap();
        self.finish_progress_internal(&mut bars, name, message);
    }
    
    /// 内部完成进度条处理
    fn finish_progress_internal(&self, bars: &mut HashMap<String, ProgressBar>, name: &str, message: Option<&str>) {
        if let Some(bar) = bars.remove(name) {
            if let Some(msg) = message {
                bar.finish_with_message(msg.to_string());
            } else {
                bar.finish();
            }
            
            debug!("完成进度条: {}", name);
            
            // 给多进度条一点时间刷新
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    
    /// 关闭所有进度条
    pub fn close_all_progress_bars(&self, message: &str) {
        if !self.show_progress {
            return;
        }
        
        let mut bars = self.progress_bars.lock().unwrap();
        
        for (name, bar) in bars.drain() {
            bar.finish_with_message(message.to_string());
            debug!("关闭进度条: {}", name);
        }
    }
    
    /// 检查是否存在指定名称的进度条
    pub fn has_progress_bar(&self, name: &str) -> bool {
        let bars = self.progress_bars.lock().unwrap();
        bars.contains_key(name)
    }
    
    /// 获取进度条
    pub fn get_progress_bar(&self, name: &str) -> Option<ProgressBar> {
        let bars = self.progress_bars.lock().unwrap();
        bars.get(name).cloned()
    }
    
    /// 暂停进度条
    pub fn pause_progress(&self, name: &str) {
        if !self.show_progress {
            return;
        }
        
        let bars = self.progress_bars.lock().unwrap();
        if let Some(bar) = bars.get(name) {
            bar.suspend(|| {
                debug!("暂停进度条: {}", name);
            });
        }
    }
    
    /// 恢复进度条
    pub fn resume_progress(&self, name: &str) {
        if !self.show_progress {
            return;
        }
        
        let bars = self.progress_bars.lock().unwrap();
        if let Some(bar) = bars.get(name) {
            bar.reset();
            debug!("恢复进度条: {}", name);
        }
    }
    
    /// 生成动画等待进度条
    pub fn create_spinner(&self, name: &str, prefix: &str, message: &str) -> Option<ProgressBar> {
        if !self.show_progress {
            return None;
        }
        
        let mut bars = self.progress_bars.lock().unwrap();
        
        // 如果已经存在，先删除旧的
        if bars.contains_key(name) {
            self.finish_progress_internal(&mut bars, name, None);
        }
        
        let spinner = self.multi_progress.add(ProgressBar::new_spinner());
        
        // 设置样式
        spinner.set_style(
            ProgressStyle::with_template(
                "{prefix:.bold.dim} {spinner} {wide_msg}"
            )
            .unwrap()
        );
        
        // 设置前缀
        spinner.set_prefix(prefix.to_string());
        
        // 设置初始描述
        spinner.set_message(message.to_string());
        
        // 启用spinner
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));
        
        // 存储进度条
        bars.insert(name.to_string(), spinner.clone());
        
        debug!("创建spinner: {}", name);
        
        Some(spinner)
    }
}

impl Drop for ProgressManager {
    fn drop(&mut self) {
        // 确保所有进度条都已完成
        self.close_all_progress_bars("已关闭");
    }
}