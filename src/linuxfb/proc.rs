use std::fs::File;
use std::io::{BufReader, BufRead};

pub fn devices() -> std::io::Result<impl Iterator<Item = Device>> {
    Ok(parse_devices(BufReader::new(File::open("/proc/devices")?)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Character,
    Block,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Device {
    pub kind: DeviceKind,
    pub major: u32,
    pub driver: String,
}

impl Device {
    pub fn parse(kind: DeviceKind, line: String) -> Option<Device> {
        let mut parts = line.split_whitespace();
        let major_str = parts.next()?;
        let driver = parts.next()?;
        
        // 确保没有多余的部分，且 major 是数字
        if let Ok(major) = major_str.parse::<u32>() {
             Some(Device {
                kind,
                major,
                driver: driver.to_string(),
            })
        } else {
            None
        }
    }
}

fn parse_device_line(current_kind: &mut DeviceKind, line: String) -> Option<Option<Device>> {
    if line.starts_with("Character devices:") {
        *current_kind = DeviceKind::Character;
        Some(None)
    } else if line.starts_with("Block devices:") {
        *current_kind = DeviceKind::Block;
        Some(None)
    } else if line.trim().is_empty() {
        Some(None)
    } else {
        Some(Device::parse(*current_kind, line))
    }
}

pub fn parse_devices(input: impl BufRead) -> impl Iterator<Item = Device> {
    input
        .lines()
        // 修复：使用 filter_map 忽略读取错误的行，防止 panic
        .filter_map(Result::ok) 
        .scan(DeviceKind::Character, parse_device_line)
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_devices() {
        let input = "
Character devices:
  1 mem
  4 tty
249 rtc

Block devices:
  7 loop
  9 md
";
        let mut devices = parse_devices(std::io::Cursor::new(input));
        assert_eq!(devices.next(), Some(Device { kind: DeviceKind::Character, major: 1, driver: String::from("mem") }));
        assert_eq!(devices.next(), Some(Device { kind: DeviceKind::Character, major: 4, driver: String::from("tty") }));
        assert_eq!(devices.next(), Some(Device { kind: DeviceKind::Character, major: 249, driver: String::from("rtc") }));
        assert_eq!(devices.next(), Some(Device { kind: DeviceKind::Block, major: 7, driver: String::from("loop") }));
        assert_eq!(devices.next(), Some(Device { kind: DeviceKind::Block, major: 9, driver: String::from("md") }));
        assert_eq!(devices.next(), None);
    }
}