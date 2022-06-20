use self::value::FunctionValue;

use super::{
    interp::Interp,
    vm::{
        byte_seq::{ByteSeq, PutValue},
        instruction::Instruction,
        stack::Stack,
        value::{Array, Function, Pointer, Value},
    },
};
use colored::Colorize;
use std::{fmt::Display, ptr};

pub mod byte_seq;
mod cast;
pub mod display;
mod index;
pub mod instruction;
mod intrinsics;
mod stack;
pub mod value;

const FRAMES_MAX: usize = 64;
const STACK_MAX: usize = FRAMES_MAX * (std::u8::MAX as usize) + 1;

pub type Constants = Vec<Value>;
pub type Globals = Vec<Value>;

#[derive(Debug, Clone)]
pub struct StackFrame {
    func: *const Function,
    stack_slot: usize,
    ip: usize,
}

impl StackFrame {
    pub fn new(func: *const Function, slot: usize) -> Self {
        Self {
            func,
            stack_slot: slot,
            ip: 0,
        }
    }

    #[inline]
    pub fn func(&self) -> &Function {
        debug_assert!(!self.func.is_null());
        unsafe { &*self.func }
    }
}

impl Display for StackFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{:06}\t{}>", self.ip, self.func().name)
    }
}

macro_rules! binary_op {
    ($vm:expr, $op:tt) => {{
        let b = $vm.stack.pop();
        let a = $vm.stack.pop();

        match (&a, &b) {
            (Value::I8(a), Value::I8(b)) => $vm.stack.push(Value::I8(a $op b)),
            (Value::I16(a), Value::I16(b)) => $vm.stack.push(Value::I16(a $op b)),
            (Value::I32(a), Value::I32(b)) => $vm.stack.push(Value::I32(a $op b)),
            (Value::I64(a), Value::I64(b)) => $vm.stack.push(Value::I64(a $op b)),
            (Value::Int(a), Value::Int(b)) => $vm.stack.push(Value::Int(a $op b)),
            (Value::U8(a), Value::U8(b)) => $vm.stack.push(Value::U8(a $op b)),
            (Value::U16(a), Value::U16(b)) => $vm.stack.push(Value::U16(a $op b)),
            (Value::U32(a), Value::U32(b)) => $vm.stack.push(Value::U32(a $op b)),
            (Value::U64(a), Value::U64(b)) => $vm.stack.push(Value::U64(a $op b)),
            (Value::Uint(a), Value::Uint(b)) => $vm.stack.push(Value::Uint(a $op b)),
            (Value::F32(a), Value::F32(b)) => $vm.stack.push(Value::F32(a $op b)),
            (Value::F64(a), Value::F64(b)) => $vm.stack.push(Value::F64(a $op b)),
            _=> panic!("invalid types in binary operation `{}` : `{}` and `{}`", stringify!($op), a.to_string() ,b.to_string())
        }

        $vm.next();
    }};
}

macro_rules! binary_op_int {
    ($vm:expr, $op:tt) => {{
        let b = $vm.stack.pop();
        let a = $vm.stack.pop();

        match (&a, &b) {
            (Value::I8(a), Value::I8(b)) => $vm.stack.push(Value::I8(a $op b)),
            (Value::I16(a), Value::I16(b)) => $vm.stack.push(Value::I16(a $op b)),
            (Value::I32(a), Value::I32(b)) => $vm.stack.push(Value::I32(a $op b)),
            (Value::I64(a), Value::I64(b)) => $vm.stack.push(Value::I64(a $op b)),
            (Value::Int(a), Value::Int(b)) => $vm.stack.push(Value::Int(a $op b)),
            (Value::U8(a), Value::U8(b)) => $vm.stack.push(Value::U8(a $op b)),
            (Value::U16(a), Value::U16(b)) => $vm.stack.push(Value::U16(a $op b)),
            (Value::U32(a), Value::U32(b)) => $vm.stack.push(Value::U32(a $op b)),
            (Value::U64(a), Value::U64(b)) => $vm.stack.push(Value::U64(a $op b)),
            (Value::Uint(a), Value::Uint(b)) => $vm.stack.push(Value::Uint(a $op b)),
            _=> panic!("invalid types in binary operation `{}` : `{}` and `{}`", stringify!($op), a.to_string() ,b.to_string())
        }

        $vm.next();
    }};
}

macro_rules! comp_op {
    ($vm:expr, $op:tt) => {
        let b = $vm.stack.pop();
        let a = $vm.stack.pop();

        match (&a, &b) {
            (Value::Bool(a), Value::Bool(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::I8(a), Value::I8(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::I16(a), Value::I16(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::I32(a), Value::I32(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::I64(a), Value::I64(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::Int(a), Value::Int(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::U8(a), Value::U8(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::U16(a), Value::U16(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::U32(a), Value::U32(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::U64(a), Value::U64(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::Uint(a), Value::Uint(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::F32(a), Value::F32(b)) => $vm.stack.push(Value::Bool(a $op b)),
            (Value::F64(a), Value::F64(b)) => $vm.stack.push(Value::Bool(a $op b)),
            _ => panic!("invalid types in compare operation `{}` and `{}`", a.to_string() ,b.to_string())
        }

        $vm.next();
    };
}

macro_rules! logic_op {
    ($vm:expr, $op:tt) => {
        let b = $vm.stack.pop();
        let a = $vm.stack.pop();

        $vm.stack.push(Value::Bool(a.into_bool() $op b.into_bool()));

        $vm.next();
    };
}

pub struct VM<'vm> {
    pub interp: &'vm mut Interp,
    pub stack: Stack<Value, STACK_MAX>,
    pub frames: Stack<StackFrame, FRAMES_MAX>,
    pub frame: *mut StackFrame,
    // pub bytecode: Bytecode<'vm>,
}

impl<'vm> VM<'vm> {
    pub fn new(interp: &'vm mut Interp) -> Self {
        Self {
            interp,
            stack: Stack::new(),
            frames: Stack::new(),
            frame: ptr::null_mut(),
        }
    }

    pub fn run_func(&'vm mut self, function: Function) -> Value {
        self.push_frame(&function as *const Function);
        self.run_inner()
    }

    fn run_inner(&'vm mut self) -> Value {
        loop {
            let frame = self.frame();
            let inst = frame.func().code.instructions[frame.ip];

            // self.trace(&inst, TraceLevel::Full);
            // std::thread::sleep(std::time::Duration::from_millis(10));

            match inst {
                Instruction::Noop => {
                    self.next();
                }
                Instruction::Pop => {
                    self.stack.pop();
                    self.next();
                }
                Instruction::PushConst(addr) => {
                    self.stack.push(self.get_const(addr).clone());
                    self.next();
                }
                Instruction::Add => {
                    binary_op!(self, +);
                }
                Instruction::Sub => {
                    binary_op!(self, -);
                }
                Instruction::Mul => {
                    binary_op!(self, *);
                }
                Instruction::Div => {
                    binary_op!(self, /);
                }
                Instruction::Rem => {
                    binary_op!(self, %);
                }
                Instruction::Neg => {
                    match self.stack.pop() {
                        Value::Int(v) => self.stack.push(Value::Int(-v)),
                        value => panic!("invalid value {}", value.to_string()),
                    }
                    self.next();
                }
                Instruction::Not => {
                    let result = match self.stack.pop() {
                        Value::I8(v) => Value::I8(!v),
                        Value::I16(v) => Value::I16(!v),
                        Value::I32(v) => Value::I32(!v),
                        Value::I64(v) => Value::I64(!v),
                        Value::Int(v) => Value::Int(!v),
                        Value::U8(v) => Value::U8(!v),
                        Value::U16(v) => Value::U16(!v),
                        Value::U32(v) => Value::U32(!v),
                        Value::U64(v) => Value::U64(!v),
                        Value::Uint(v) => Value::Uint(!v),
                        Value::Bool(v) => Value::Bool(!v),
                        v => panic!("invalid value {}", v.to_string()),
                    };
                    self.stack.push(result);
                    self.next();
                }
                Instruction::Deref => {
                    match self.stack.pop() {
                        Value::Pointer(ptr) => {
                            let value = unsafe { ptr.deref_value() };
                            self.stack.push(value);
                        }
                        value => panic!("invalid value {}", value.to_string()),
                    }
                    self.next();
                }
                Instruction::Eq => {
                    comp_op!(self, ==);
                }
                Instruction::Neq => {
                    comp_op!(self, !=);
                }
                Instruction::Lt => {
                    comp_op!(self, <);
                }
                Instruction::LtEq => {
                    comp_op!(self, <=);
                }
                Instruction::Gt => {
                    comp_op!(self, >);
                }
                Instruction::GtEq => {
                    comp_op!(self, >=);
                }
                Instruction::And => {
                    logic_op!(self, &&);
                }
                Instruction::Or => {
                    logic_op!(self, ||);
                }
                Instruction::Shl => {
                    binary_op_int!(self, <<)
                }
                Instruction::Shr => {
                    binary_op_int!(self, >>);
                }
                Instruction::Xor => {
                    binary_op_int!(self, ^);
                }
                Instruction::Jmp(offset) => {
                    self.jmp(offset);
                }
                Instruction::Jmpt(offset) => {
                    if self.stack.pop().into_bool() {
                        self.jmp(offset);
                    } else {
                        self.next();
                    }
                }
                Instruction::Jmpf(offset) => {
                    if !self.stack.pop().into_bool() {
                        self.jmp(offset);
                    } else {
                        self.next();
                    }
                }
                Instruction::Return => {
                    let frame = self.frames.pop();
                    let return_value = self.stack.pop();

                    if self.frames.is_empty() {
                        break return_value;
                    } else {
                        self.stack
                            .truncate(frame.stack_slot - frame.func().arg_types.len());
                        self.frame = self.frames.last_mut() as _;
                        self.stack.push(return_value);
                        self.next();
                    }
                }
                Instruction::Call(arg_count) => match self.stack.pop() {
                    Value::Function(addr) => match self.interp.get_function(addr.id).unwrap() {
                        FunctionValue::Orphan(function) => {
                            self.push_frame(function as *const Function);
                        }
                        FunctionValue::Extern(function) => {
                            let mut values = (0..arg_count)
                                .into_iter()
                                .map(|_| self.stack.pop())
                                .collect::<Vec<Value>>();

                            values.reverse();

                            let function = function.clone();
                            let vm_ptr = self as *mut VM;

                            let result = unsafe { self.interp.ffi.call(vm_ptr, function, values) };
                            self.stack.push(result);

                            self.next();
                        }
                    },
                    Value::Intrinsic(intrinsic) => self.dispatch_intrinsic(intrinsic),
                    value => panic!("tried to call uncallable value `{}`", value.to_string()),
                },
                Instruction::GetGlobal(slot) => {
                    match self.interp.globals.get(slot as usize) {
                        Some(value) => self.stack.push(value.clone()),
                        None => panic!("undefined global `{}`", slot),
                    }
                    self.next();
                }
                Instruction::GetGlobalPtr(slot) => {
                    match self.interp.globals.get_mut(slot as usize) {
                        Some(value) => self.stack.push(Value::Pointer(value.into())),
                        None => panic!("undefined global `{}`", slot),
                    }
                    self.next();
                }
                Instruction::SetGlobal(slot) => {
                    self.interp.globals[slot as usize] = self.stack.pop();
                    self.next();
                }
                Instruction::Peek(slot) => {
                    let slot = self.frame().stack_slot as isize + slot as isize;
                    let value = self.stack.get(slot as usize).clone();
                    self.stack.push(value);
                    self.next();
                }
                Instruction::PeekPtr(slot) => {
                    let slot = self.frame().stack_slot as isize + slot as isize;
                    let value = self.stack.get_mut(slot as usize);
                    let value = Value::Pointer(value.into());
                    self.stack.push(value);
                    self.next();
                }
                Instruction::SetLocal(slot) => {
                    let slot = self.frame().stack_slot as isize + slot as isize;
                    let value = self.stack.pop();
                    self.stack.set(slot as usize, value);
                    self.next();
                }
                Instruction::Index => {
                    let index = self.stack.pop().into_uint();
                    let value = self.stack.pop();
                    self.index(value, index);
                    self.next();
                }
                Instruction::IndexPtr => {
                    let index = self.stack.pop().into_uint();
                    let value = self.stack.pop();
                    self.index_ptr(value, index);
                    self.next();
                }
                Instruction::Offset => {
                    let index = self.stack.pop().into_uint();
                    let value = self.stack.pop();
                    self.offset(value, index);
                    self.next();
                }
                Instruction::ConstIndex(index) => {
                    let value = self.stack.pop();
                    self.index(value, index as usize);
                    self.next();
                }
                Instruction::ConstIndexPtr(index) => {
                    let value = self.stack.pop();
                    self.index_ptr(value, index as usize);
                    self.next();
                }
                Instruction::Assign => {
                    let lvalue = self.stack.pop().into_pointer();
                    let rvalue = self.stack.pop();
                    unsafe { lvalue.write_value(rvalue) }
                    self.next();
                }
                Instruction::Cast(cast) => {
                    self.cast_inst(cast);
                    self.next();
                }
                Instruction::AggregateAlloc => {
                    self.stack.push(Value::unit());
                    self.next();
                }
                Instruction::AggregatePush => {
                    let value = self.stack.pop();
                    let aggregate = self.stack.peek_mut(0).as_aggregate_mut();
                    aggregate.elements.push(value);
                    self.next();
                }
                Instruction::ArrayAlloc(size) => {
                    let ty = self.stack.pop().into_type();
                    self.stack.push(Value::Array(Array {
                        bytes: ByteSeq::new(size as usize),
                        ty,
                    }));
                    self.next();
                }
                Instruction::ArrayPut(pos) => {
                    let value = self.stack.pop();
                    let array = self.stack.peek_mut(0).as_array_mut();

                    let offset_bytes = array.bytes.offset_mut(pos as usize);
                    offset_bytes.put_value(&value);

                    self.next();
                }
                Instruction::ArrayFill(size) => {
                    let value = self.stack.pop();
                    let array = self.stack.peek_mut(0).as_array_mut();

                    for _ in 0..size {
                        array.bytes.put_value(&value);
                    }

                    self.next();
                }
                Instruction::Copy(offset) => {
                    let value = self.stack.peek(offset as usize).clone();
                    self.stack.push(value);
                    self.next();
                }
                Instruction::Roll(offset) => {
                    let value = self.stack.take(offset as usize);
                    self.stack.push(value);
                    self.next();
                }
                Instruction::Increment => {
                    let ptr = self.stack.pop().into_pointer();
                    unsafe {
                        match ptr {
                            Pointer::I8(v) => *v += 1,
                            Pointer::I16(v) => *v += 1,
                            Pointer::I32(v) => *v += 1,
                            Pointer::I64(v) => *v += 1,
                            Pointer::Int(v) => *v += 1,
                            Pointer::U8(v) => *v += 1,
                            Pointer::U16(v) => *v += 1,
                            Pointer::U32(v) => *v += 1,
                            Pointer::U64(v) => *v += 1,
                            Pointer::Uint(v) => *v += 1,
                            _ => panic!("invalid pointer in increment {:?}", ptr),
                        }
                    }
                    self.next();
                }
                Instruction::Panic => {
                    // Note (Ron): the panic message is a slice, which is an aggregate in the VM
                    let message = self.stack.pop().into_aggregate();
                    let ptr = message.elements[0].clone().into_pointer().as_inner_raw();
                    let len = message.elements[1].clone().into_uint();
                    let slice = unsafe { std::slice::from_raw_parts(ptr as *mut u8, len) };
                    let str = std::str::from_utf8(slice).unwrap();

                    // TODO: instead of using Rust's panic, we should be using our own panic function
                    panic!("{}", str);
                }
                Instruction::Halt => {
                    let result = self.stack.pop();
                    break result;
                }
            }
        }
    }

    #[inline]
    pub fn push_frame(&mut self, func: *const Function) {
        debug_assert!(!func.is_null());

        let stack_slot = self.stack.len();

        let locals = unsafe { &*func }.code.locals;
        for _ in 0..locals {
            self.stack.push(Value::unit());
        }

        self.frames.push(StackFrame::new(func, stack_slot));

        self.frame = self.frames.last_mut() as _;
    }

    #[inline]
    pub fn frame(&self) -> &StackFrame {
        debug_assert!(!self.frame.is_null());
        unsafe { &*self.frame }
    }

    #[inline]
    pub fn frame_mut(&mut self) -> &mut StackFrame {
        debug_assert!(!self.frame.is_null());
        unsafe { &mut *self.frame }
    }

    #[inline]
    pub fn next(&mut self) {
        self.frame_mut().ip += 1;
    }

    #[inline]
    pub fn get_const(&self, addr: u32) -> &Value {
        self.interp.constants.get(addr as usize).unwrap()
    }

    #[inline]
    pub fn jmp(&mut self, offset: i32) {
        let new_inst_pointer = self.frame().ip as isize + offset as isize;
        self.frame_mut().ip = new_inst_pointer as usize;
    }

    #[allow(unused)]
    pub fn trace(&self, inst: &Instruction, level: TraceLevel) {
        let frame = self.frame();

        match level {
            TraceLevel::None => (),
            TraceLevel::Minimal => {
                println!(
                    "{:06}\t{:<20}{}",
                    frame.ip,
                    inst.to_string().bold(),
                    format!("[stack items: {}]", self.stack.len()).bright_cyan()
                );
            }
            TraceLevel::Full => {
                println!("{:06}\t{}", frame.ip, inst.to_string().bold());

                print!("\t[");

                let frame_slot = frame.stack_slot;

                for (index, value) in self.stack.iter().enumerate() {
                    print!(
                        "{}",
                        if index == frame_slot {
                            // frame slot
                            value.to_string().bright_yellow()
                        } else if index > frame_slot - frame.func().arg_types.len()
                            && index <= frame_slot + frame.func().code.locals as usize
                        {
                            // local value
                            value.to_string().bright_magenta()
                        } else {
                            // any other value
                            value.to_string().bright_cyan()
                        }
                    );

                    if index < self.stack.len() - 1 {
                        print!(", ");
                    }
                }

                println!("] ({})\n", self.stack.len());
            }
        }
    }
}

#[allow(dead_code)]
pub enum TraceLevel {
    None,
    Minimal,
    Full,
}