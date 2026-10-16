//! Declarative opcode contract. Reserved and legacy spellings are deliberately absent.
#[derive(Clone, Copy, Debug)]
pub struct Opcode {
    pub name: &'static str,
    pub min_operands: usize,
    pub max_operands: usize,
    pub payload: &'static str,
    /// 0 = none, 1 = required, 2 = optional.
    pub result: u8,
    pub terminator: bool,
    pub min_edges: usize,
    pub max_edges: usize,
}
macro_rules! table {
    ($( $n:literal,$a:expr,$b:expr,$p:literal,$r:expr,$t:expr,$e:expr,$f:expr; )*) => {
        pub const OPCODES: &[Opcode] = &[$(Opcode{name:$n,min_operands:$a,max_operands:$b,payload:$p,result:$r,terminator:$t,min_edges:$e,max_edges:$f},)*];
    }
}
table! {
    "COPY",1,1,"",1,false,0,0;
    "PHI",0,0,"phi",1,false,0,0;
    "SELECT",3,3,"",1,false,0,0;
    "UNDEF",0,0,"",1,false,0,0;
    "HAVOC",0,0,"",1,false,0,0;
    "ADD",2,2,"binary",1,false,0,0;
    "SUB",2,2,"binary",1,false,0,0;
    "MUL",2,2,"binary",1,false,0,0;
    "DIV",2,2,"binary",1,false,0,0;
    "REM",2,2,"binary",1,false,0,0;
    "NEG",1,1,"binary",1,false,0,0;
    "EQ",2,2,"",1,false,0,0;
    "NE",2,2,"",1,false,0,0;
    "LT",2,2,"",1,false,0,0;
    "LE",2,2,"",1,false,0,0;
    "GT",2,2,"",1,false,0,0;
    "GE",2,2,"",1,false,0,0;
    "BIT_AND",2,2,"",1,false,0,0;
    "BIT_OR",2,2,"",1,false,0,0;
    "XOR",2,2,"",1,false,0,0;
    "BIT_NOT",1,1,"",1,false,0,0;
    "SHL",2,2,"binary",1,false,0,0;
    "SHR",2,2,"binary",1,false,0,0;
    "LOGICAL_NOT",1,1,"",1,false,0,0;
    "SIGN_EXTEND",1,1,"",1,false,0,0;
    "ZERO_EXTEND",1,1,"",1,false,0,0;
    "TRUNCATE",1,1,"",1,false,0,0;
    "BITCAST",1,1,"",1,false,0,0;
    "EXTRACT_FIELD",1,1,"field",1,false,0,0;
    "INSERT_FIELD",2,2,"field",1,false,0,0;
    "INDEX",2,2,"",1,false,0,0;
    "LENGTH",1,1,"",1,false,0,0;
    "GET_TAG",1,1,"",1,false,0,0;
    "ALLOC",0,0,"memory",1,false,0,0;
    "LOAD",1,1,"memory",1,false,0,0;
    "STORE",2,2,"memory",0,false,0,0;
    "FIELD_ADDR",1,1,"field",1,false,0,0;
    "INDEX_ADDR",2,2,"",1,false,0,0;
    "ADDRESS_OF",1,1,"memory",1,false,0,0;
    "MEMORY_BARRIER",0,0,"",0,false,0,0;
    "CALL",1,usize::MAX,"call",2,false,0,0;
    "NEW_OBJECT",0,0,"type_test",1,false,0,0;
    "TYPE_TEST",1,1,"type_test",1,false,0,0;
    "DOWNCAST",1,1,"type_test",1,false,0,0;
    "MAKE_INTERFACE",1,1,"type_test",1,false,0,0;
    "MAKE_CLOSURE",1,usize::MAX,"",1,false,0,0;
    "DEFER_PUSH",1,usize::MAX,"call",0,false,0,0;
    "RUN_DEFERS",0,0,"",0,false,0,0;
    "RECOVER",0,0,"",1,false,0,0;
    "EMIT_EFFECT",0,usize::MAX,"extension",0,false,0,0;
    "UNKNOWN_EFFECT",0,usize::MAX,"opaque",0,false,0,0;
    "OPAQUE_OP",0,usize::MAX,"opaque",2,false,0,0;
    "SPAWN",1,usize::MAX,"call",0,false,0,0;
    "JOIN",0,usize::MAX,"",0,false,0,0;
    "CHANNEL_SEND",2,2,"",0,false,0,0;
    "CHANNEL_RECV",1,1,"",1,false,0,0;
    "LOCK",1,1,"",0,false,0,0;
    "UNLOCK",1,1,"",0,false,0,0;
    "RETURN",0,usize::MAX,"",0,true,0,0;
    "JUMP",0,0,"",0,true,1,1;
    "BRANCH",1,1,"",0,true,2,2;
    "SWITCH",1,1,"",0,true,2,usize::MAX;
    "INVOKE",1,usize::MAX,"call",2,true,2,usize::MAX;
    "THROW",1,1,"",0,true,0,0;
    "PANIC",1,1,"",0,true,0,0;
    "UNREACHABLE",0,0,"",0,true,0,0;
}
pub fn lookup(name: &str) -> Option<&'static Opcode> {
    OPCODES.iter().find(|op| op.name == name)
}
