use std::fmt;

/// 参数化值 —— 所有用户输入必经此枚举，杜绝 SQL 注入。
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Null,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Text(String),
    Bytes(Vec<u8>),
    #[cfg(feature = "chrono")]
    DateTime(chrono::NaiveDateTime),
    #[cfg(feature = "chrono")]
    DateTimeTz(chrono::DateTime<chrono::Utc>),
}

// --- From impls ---

impl From<&str> for Param {
    fn from(s: &str) -> Self { Param::Text(s.to_owned()) }
}

impl From<String> for Param {
    fn from(s: String) -> Self { Param::Text(s) }
}

impl From<&String> for Param {
    fn from(s: &String) -> Self { Param::Text(s.clone()) }
}

impl From<bool> for Param {
    fn from(b: bool) -> Self { Param::Bool(b) }
}

impl From<i8> for Param {
    fn from(v: i8) -> Self { Param::I8(v) }
}

impl From<i16> for Param {
    fn from(v: i16) -> Self { Param::I16(v) }
}

impl From<i32> for Param {
    fn from(v: i32) -> Self { Param::I32(v) }
}

impl From<i64> for Param {
    fn from(v: i64) -> Self { Param::I64(v) }
}

impl From<f32> for Param {
    fn from(v: f32) -> Self { Param::F32(v) }
}

impl From<f64> for Param {
    fn from(v: f64) -> Self { Param::F64(v) }
}

impl<T: Into<Param>> From<Option<T>> for Param {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(val) => val.into(),
            None => Param::Null,
        }
    }
}

#[cfg(feature = "chrono")]
impl From<chrono::NaiveDateTime> for Param {
    fn from(dt: chrono::NaiveDateTime) -> Self { Param::DateTime(dt) }
}

#[cfg(feature = "chrono")]
impl From<chrono::DateTime<chrono::Utc>> for Param {
    fn from(dt: chrono::DateTime<chrono::Utc>) -> Self { Param::DateTimeTz(dt) }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Param::Null => write!(f, "NULL"),
            Param::Bool(b) => write!(f, "{b}"),
            Param::I8(v) => write!(f, "{v}"),
            Param::I16(v) => write!(f, "{v}"),
            Param::I32(v) => write!(f, "{v}"),
            Param::I64(v) => write!(f, "{v}"),
            Param::F32(v) => write!(f, "{v}"),
            Param::F64(v) => write!(f, "{v}"),
            Param::Text(s) => write!(f, "{s}"),
            Param::Bytes(_) => write!(f, "<bytes>"),
            #[cfg(feature = "chrono")]
            Param::DateTime(dt) => write!(f, "{dt}"),
            #[cfg(feature = "chrono")]
            Param::DateTimeTz(dt) => write!(f, "{dt}"),
        }
    }
}
