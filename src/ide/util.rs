use crate::span::Span;
use crate::workspace::Workspace;

#[inline]
pub fn is_offset_in_span_and_root_module(workspace: &Workspace, offset: usize, span: Span) -> bool {
    span.contains(offset)
        && workspace
            .find_module_id_by_file_id(span.file_id)
            .map_or(false, |module_id| module_id == workspace.root_module_id)
}

#[inline]
pub fn write<T>(value: &T)
where
    T: ?Sized + serde::Serialize,
{
    println!("{}", serde_json::to_string(value).unwrap())
}

#[inline]
pub fn write_null() {
    println!("null")
}
