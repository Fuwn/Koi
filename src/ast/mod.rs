use std::collections::HashMap;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum UnaryOp {
    Neg,
    Not,

    PreDec,
    PreInc,

    PostDec,
    PostInc,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BinaryOp {
    Sum,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,

    Great,
    Less,

    Equal,

    And,
    Or,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Literal(Value),
    Unary(UnaryOp, Box<Expr>),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),

    Interp {
        strings: Vec<String>,
        exprs: Vec<Expr>,
    },

    Get(String),
    Set(String, Box<Expr>),

    GetField {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    SetField {
        base: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
}

#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    Num(f64),
    String(String),
    Bool(bool),
    Vec(Vec<Value>),
    Dict(HashMap<Value, Value>),
}
