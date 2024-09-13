use std::fmt::Debug;
use std::str::FromStr;

pub struct OptionT<T>(pub Option<T>);

impl<T> FromStr for OptionT<T>
where
    T: FromStr,    // T 必须实现 FromStr
    T::Err: Debug, // 错误需要实现 Debug 以便输出调试信息
{
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        // 如果输入的是 "None" (忽略大小写)，返回 None
        if lower == "none" || lower == "null" || lower == "no" || lower == "n" {
            Ok(OptionT(None))
        } else {
            // 否则尝试解析为 Some(T)
            match s.parse::<T>() {
                Ok(value) => Ok(OptionT(Some(value))),
                Err(e) => Err(format!("Failed to parse input: {:?}", e)),
            }
        }
    }
}
