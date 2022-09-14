pub const SYM_UNIT: &str = "unit";
pub const SYM_BOOL: &str = "bool";
pub const SYM_I8: &str = "i8";
pub const SYM_I16: &str = "i16";
pub const SYM_I32: &str = "i32";
pub const SYM_I64: &str = "i64";
pub const SYM_INT: &str = "int";
pub const SYM_U8: &str = "u8";
pub const SYM_U16: &str = "u16";
pub const SYM_U32: &str = "u32";
pub const SYM_U64: &str = "u64";
pub const SYM_UINT: &str = "uint";
pub const SYM_F16: &str = "f16";
pub const SYM_F32: &str = "f32";
pub const SYM_F64: &str = "f64";
pub const SYM_FLOAT: &str = "float";
pub const SYM_STR: &str = "str";
pub const SYM_NEVER: &str = "never";

pub const SYM_SELF: &str = "self";
pub const SYM_SUPER: &str = "super";

pub const SYM_TRACK_CALLER_LOCATION_PARAM: &str = "track_caller@location";

pub fn is_implicitly_generated_param(name: &str) -> bool {
    name == SYM_TRACK_CALLER_LOCATION_PARAM
}
