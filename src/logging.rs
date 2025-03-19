use fern::colors::{Color, ColoredLevelConfig};
use log::LevelFilter;
use std::path::Path;

/// 设置应用日志，类似于Python版本的setup_logging
pub fn setup_logging(log_file: Option<&Path>) -> std::result::Result<(), fern::InitError> {
    // 配置颜色
    let colors = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::Blue)
        .trace(Color::BrightBlack);

    // 基本logger配置
    let mut logger = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(LevelFilter::Info)
        .chain(std::io::stdout());

    // 如果指定了日志文件，添加文件输出
    if let Some(log_path) = log_file {
        // 确保父目录存在
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        logger = logger.chain(fern::log_file(log_path)?);
    }

    // 应用配置
    logger.apply()?;

    Ok(())
}