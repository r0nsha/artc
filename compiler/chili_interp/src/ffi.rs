use crate::value::{ForeignFunc, Value};
use chili_ast::ty::*;
use libffi::low::{
    call, ffi_abi_FFI_DEFAULT_ABI, ffi_cif, ffi_type, prep_cif, prep_cif_var, types, CodePtr,
};
use std::{ffi::c_void, mem};

pub unsafe fn call_foreign_func(func: ForeignFunc, args: Vec<Value>) -> Value {
    let lib = load_lib(&func.lib_path);
    let symbol = load_symbol(&lib, &func.name);
    let cif = create_cif(&func, args.len());
    let call_result = do_ffi_call(symbol, args, cif);
    convert_result_to_value(call_result, func.ret_ty)
}

unsafe fn load_lib(lib_path: &str) -> libloading::Library {
    let lib_name = match lib_path {
        "c" | "C" | "libucrt" => "msvcrt".to_string(), // TODO: this depends on the platform,
        _ => lib_path.to_string(),
    };
    libloading::Library::new(format!("{}.dll", lib_name)).unwrap()
}

unsafe fn load_symbol<'a>(
    lib: &'a libloading::Library,
    symbol: &str,
) -> libloading::Symbol<'a, *mut c_void> {
    lib.get(symbol.as_bytes()).unwrap()
}

unsafe fn create_cif(func: &ForeignFunc, arg_count: usize) -> ffi_cif {
    let mut cif = ffi_cif::default();
    let abi = ffi_abi_FFI_DEFAULT_ABI;

    let ret: *mut ffi_type = &mut func.ret_ty.as_ffi_type();

    let mut params = func
        .param_tys
        .iter()
        .map(|param| &mut param.as_ffi_type() as *mut _)
        .collect::<Vec<*mut ffi_type>>();

    if func.variadic {
        prep_cif_var(
            &mut cif,
            abi,
            params.len(),
            arg_count,
            ret,
            params.as_mut_ptr(),
        )
        .unwrap()
    } else {
        prep_cif(&mut cif, abi, params.len(), ret, params.as_mut_ptr()).unwrap()
    }

    cif
}

unsafe fn do_ffi_call(
    symbol: libloading::Symbol<*mut c_void>,
    args: Vec<Value>,
    mut cif: ffi_cif,
) -> *mut c_void {
    let code_ptr = CodePtr::from_ptr(*symbol);
    let args = args
        .iter()
        .map(|arg| unsafe { arg.as_ffi_arg() })
        .collect::<Vec<*mut c_void>>()
        .as_mut_ptr();

    let mut result = mem::MaybeUninit::<c_void>::uninit();

    libffi::raw::ffi_call(
        &mut cif as *mut _,
        Some(*code_ptr.as_safe_fun()),
        result.as_mut_ptr() as *mut c_void,
        args,
    );

    result.assume_init_mut()
}

unsafe fn convert_result_to_value(result: *mut c_void, ret_ty: TyKind) -> Value {
    match ret_ty {
        TyKind::Never | TyKind::Unit => Value::unit(),
        TyKind::Bool => Value::Bool(*(result as *const bool)),
        TyKind::Int(_) | TyKind::UInt(_) | TyKind::Infer(_, InferTy::AnyInt) => {
            Value::Int(*(result as *const i64))
        }
        TyKind::Float(_) | TyKind::Infer(_, InferTy::AnyFloat) => {
            Value::Float(*(result as *const f64))
        }
        TyKind::Pointer(_, _) | TyKind::MultiPointer(_, _) => Value::Ptr(result as *mut u8),
        TyKind::Fn(_) => todo!(),
        TyKind::Array(_, _) => todo!(),
        TyKind::Slice(_, _) => todo!(),
        TyKind::Tuple(_) => todo!(),
        TyKind::Struct(_) => todo!(),
        TyKind::Infer(_, InferTy::PartialTuple(_)) => todo!(),
        TyKind::Infer(_, InferTy::PartialStruct(_)) => todo!(),
        _ => panic!("invalid type {}", ret_ty),
    }
}

trait AsFfiType {
    unsafe fn as_ffi_type(&self) -> ffi_type;
}

impl AsFfiType for TyKind {
    unsafe fn as_ffi_type(&self) -> ffi_type {
        const IS_64BIT: bool = mem::size_of::<usize>() == 8;

        match self {
            TyKind::Bool => types::uint8,
            TyKind::Int(ty) => match ty {
                IntTy::I8 => types::sint8,
                IntTy::I16 => types::sint16,
                IntTy::I32 => types::sint32,
                IntTy::I64 => types::sint64,
                IntTy::Int => {
                    if IS_64BIT {
                        types::sint64
                    } else {
                        types::sint32
                    }
                }
            },
            TyKind::UInt(ty) => match ty {
                UIntTy::U8 => types::uint8,
                UIntTy::U16 => types::uint16,
                UIntTy::U32 => types::uint32,
                UIntTy::U64 => types::uint64,
                UIntTy::UInt => {
                    if IS_64BIT {
                        types::uint64
                    } else {
                        types::uint32
                    }
                }
            },
            TyKind::Float(_) => types::float,
            TyKind::Unit => todo!(),
            TyKind::Pointer(_, _) | TyKind::MultiPointer(_, _) => types::pointer,
            TyKind::Fn(_) => todo!(),
            TyKind::Array(_, _) => todo!(),
            TyKind::Slice(_, _) => todo!(),
            TyKind::Tuple(_) => todo!(),
            TyKind::Struct(_) => todo!(),
            TyKind::Infer(_, ty) => match ty {
                InferTy::AnyInt => types::sint64,
                InferTy::AnyFloat => types::float,
                InferTy::PartialStruct(_) => todo!(),
                InferTy::PartialTuple(_) => todo!(),
            },
            TyKind::Never => types::void,
            _ => panic!("invalid type {}", self),
        }
    }
}

trait AsFfiArg {
    unsafe fn as_ffi_arg(&self) -> *mut c_void;
}

impl AsFfiArg for Value {
    unsafe fn as_ffi_arg(&self) -> *mut c_void {
        match self {
            Value::Int(mut v) => &mut v as *mut _ as *mut c_void,
            Value::Bool(mut v) => &mut v as *mut _ as *mut c_void,
            Value::Float(mut v) => &mut v as *mut _ as *mut c_void,
            Value::Tuple(_) => todo!("tuple"),
            Value::Ptr(ptr) => *ptr as *mut c_void,
            Value::Slice(_) => todo!("slice"),
            Value::Func(_) => todo!("func"),
            Value::ForeignFunc(_) => todo!("foreign func"),
        }
    }
}
