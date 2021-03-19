use core::fmt;
use std::borrow::Borrow;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::panic::panic_any;
use std::rc::Rc;
use std::env;

use itertools::Itertools;

use crate::ast::{BinaryOp, Expr, Prog, Stmt, UnaryOp};
use crate::ast::Expr::Interp;
use crate::interp::stack::{Stack, Var};

mod cmd;

mod stack;

#[cfg(test)]
mod test;

pub struct Interpreter {
    stack: Stack,
    collector: Option<String>,
}

fn print(int: &mut Interpreter, args: Vec<Value>) -> Value {
    let mut res = args.iter().map(|arg| arg.to_string()).join(" ");

    if let Some(str) = &mut int.collector {
        str.push_str(&res);
        str.push_str("\n");
    } else {
        println!("{}", res);
    }

    Value::Nil
}

#[derive(Debug)]
enum Escape {
    Break,
    Continue,
    Return
}

impl Interpreter {
    pub fn new() -> Interpreter {
        let mut interpreter = Interpreter {
            stack: Stack::new(),
            collector: None,
        };
        interpreter.init_native_funcs();
        interpreter.import_os_env();
        interpreter
    }

    pub fn run(&mut self, prog: Prog) {
        for stmt in prog.into_iter() {
            self.run_stmt(stmt).expect("escape bubbled up to top level");
        }
    }

    pub fn do_collect(&mut self) {
        self.collector = Some(String::new());
    }

    fn init_native_funcs(&mut self) {
        self.stack.def("print".to_string(), Value::Func(Func::Native {
            name: "print".to_string(),
            func: print,
        }));
    }

    fn import_os_env(&mut self) {
        for (k, v) in env::vars() {
            self.stack.def(k, Value::String(v));
        }
    }

    fn run_stmt(&mut self, stmt: Stmt) -> Result<(),Escape> {
        match stmt {
            Stmt::Cmd(cmd) => {
                let env = self.stack.os_env();

                if self.collector.is_some() {
                    let output = self.run_cmd_capture(cmd, env);
                    self.collector.as_mut().unwrap().push_str(&output);
                } else {
                    self.run_cmd_pipe(cmd, env);
                }
            }
            Stmt::Let { name, init, is_exp } => {
                let val = match init {
                    Some(expr) => self.eval(expr),
                    _ => Value::Nil,
                };

                self.stack.def(name, Var {
                    val,
                    is_exp,
                });
            }
            Stmt::Expr(expr) => {
                self.eval(expr);
            }
            Stmt::Block(stmts) => {
                self.stack.push();
                for stmt in stmts {
                    self.run_stmt(stmt)?;
                }
                self.stack.pop();
            }
            Stmt::For { lvar, rvar, iterated, each_do } => {
                let iterated = self.eval(iterated);

                match iterated {
                    Value::Range(l, r) => {
                        assert!(rvar.is_none(), "for loop with range does not need a second variable");

                        self.stack.push();
                        self.stack.def(lvar.clone(), Value::Num(l as f64));

                        for i in l..r {
                            *self.stack.get_mut(&lvar) = Value::Num(i as f64);

                            let res = self.run_stmt(*each_do.clone());
                            match &res {
                                Err(Escape::Continue) => continue,
                                Err(Escape::Break) => break,
                                _ => res?,
                            };
                        }

                        self.stack.pop();
                    }
                    Value::Vec(vec) => {
                        let rvar = rvar.expect("for loop with vec does need a second variable");

                        self.stack.push();
                        self.stack.def(lvar.clone(), Value::Nil);
                        self.stack.def(rvar.clone(), Value::Nil);

                        for (i, v) in RefCell::borrow(&vec).iter().enumerate() {
                            *self.stack.get_mut(&lvar) = Value::Num(i as f64);
                            *self.stack.get_mut(&rvar) = v.clone();

                            let res = self.run_stmt(*each_do.clone());
                            match &res {
                                Err(Escape::Continue) => continue,
                                Err(Escape::Break) => break,
                                _ => res?,
                            };
                        }

                        self.stack.pop();
                    }
                    Value::Dict(dict) => {
                        let rvar = rvar.expect("for loop with vec does need a second variable");

                        self.stack.push();
                        self.stack.def(lvar.clone(), Value::Nil);
                        self.stack.def(rvar.clone(), Value::Nil);

                        for (k, v) in RefCell::borrow(&dict).iter() {
                            *self.stack.get_mut(&lvar) = Value::String(k.clone());
                            *self.stack.get_mut(&rvar) = v.clone();

                            let res = self.run_stmt(*each_do.clone());
                            match &res {
                                Err(Escape::Continue) => continue,
                                Err(Escape::Break) => break,
                                _ => res?,
                            };
                        }

                        self.stack.pop();
                    }
                    _ => unreachable!()
                }
            }
            Stmt::While { cond, then_do } => {
                while self.eval(cond.clone()).is_truthy() {
                    let res = self.run_stmt(*then_do.clone());
                    match &res {
                        Err(Escape::Continue) => continue,
                        Err(Escape::Break) => break,
                        _ => res?,
                    };
                }
            }
            Stmt::If {cond, then_do, else_do} => {
                if self.eval(cond).is_truthy() {
                    self.run_stmt(*then_do)?;
                } else if else_do.is_some() {
                    self.run_stmt(*else_do.unwrap())?;
                }
            }

            Stmt::Continue => return Err(Escape::Continue),
            Stmt::Break => return Err(Escape::Break),

            _ => todo!()
        };
        Ok(())
    }

    fn eval(&mut self, expr: Expr) -> Value {
        match expr {
            Expr::Literal(value) => value,
            Expr::Vec(vec) => {
                let vec = vec.into_iter().map(|expr| self.eval(expr)).collect::<Vec<Value>>();
                let vec = Rc::new(RefCell::new(vec));
                Value::Vec(vec)
            }
            Expr::Dict(dict) => {
                let dict = dict.into_iter().map(|(key, expr)| (key, self.eval(expr))).collect::<HashMap<String, Value>>();
                let dict = Rc::new(RefCell::new(dict));
                Value::Dict(dict)
            }
            Expr::Cmd(cmd) => Value::String(self.run_cmd_capture(cmd, self.stack.os_env())),
            Expr::Get(name) => self.stack.get(&name).clone(),
            Expr::GetField {base, index} => {
                let base = self.eval(*base);
                let index = self.eval(*index);

                match base {
                    Value::Vec(vec) => {
                        let index = match index {
                            Value::Num(num) if num.trunc() == num => num as usize,
                            _ => panic!("bad index, want integer"),
                        };

                        RefCell::borrow(&vec)[index].clone()
                    }
                    Value::Dict(dict) => {
                        let index = match index {
                            Value::String(str) => str,
                            _ => panic!("bad index, want string"),
                        };

                        RefCell::borrow(&dict).get(&index).cloned().unwrap()
                    },
                    _ => panic!("bad get target"),
                }
            }
            Expr::Set(name, expr) => {
                let value = self.eval(*expr);
                *self.stack.get_mut(&name) = value.clone();
                value
            }
            Expr::SetField { base, index, expr } => {
                let base = self.eval(*base);
                let index = self.eval(*index);
                let value = self.eval(*expr);

                match base {
                    Value::Vec(vec) => {
                        let index = match index {
                            Value::Num(num) if num.trunc() == num => num as usize,
                            _ => panic!("bad index, want integer"),
                        };

                        vec.borrow_mut()[index] = value.clone();
                    }
                    Value::Dict(dict) => {
                        let index = match index {
                            Value::String(str) => str,
                            _ => panic!("bad index, want string"),
                        };

                        dict.borrow_mut().insert(index, value.clone());
                    },
                    _ => panic!("bad assignment target"),
                };

                value
            }
            Expr::Interp { mut strings, exprs } => {
                let mut out = String::new();

                out += &strings.remove(0);

                for expr in exprs {
                    let str = self.eval(expr).to_string();
                    out += &str;
                    out += &strings.remove(0);
                }

                Value::String(out)
            }
            Expr::Range { l, r, inclusive } => {
                let l = self.eval(*l);
                let r = self.eval(*r);

                match (l, r) {
                    // The x.trunc() == x part is to check that the numbers are integers
                    (Value::Num(l), Value::Num(r)) if l.trunc() == l && r.trunc() == r => {
                        Value::Range(l as i32, r as i32 + if inclusive { 1 } else { 0 })
                    }
                    _ => panic!("range must evaluate to integers")
                }
            }
            Expr::Binary(lhs, BinaryOp::Sum, rhs) => {
                match (self.eval(*lhs), self.eval(*rhs)) {
                    (Value::Num(lhs), Value::Num(rhs)) => Value::Num(lhs + rhs),
                    (Value::String(lhs), Value::String(rhs)) => Value::String(lhs + &rhs),
                    _ => panic!("invalid operands types for op {:?}", BinaryOp::Sum),
                }
            }
            Expr::Binary(lhs, op, rhs) if [
                BinaryOp::Sub, BinaryOp::Mul, BinaryOp::Div,
                BinaryOp::Mod, BinaryOp::Pow, BinaryOp::Less, BinaryOp::Great
            ].contains(&op) => {
                let (lhs, rhs) = match (self.eval(*lhs), self.eval(*rhs)) {
                    (Value::Num(lhs), Value::Num(rhs)) => (lhs, rhs),
                    _ => panic!("invalid operands types for op {:?}", op),
                };

                match op {
                    BinaryOp::Sub => Value::Num(lhs - rhs),
                    BinaryOp::Mul => Value::Num(lhs * rhs),
                    BinaryOp::Div => Value::Num(lhs / rhs),
                    BinaryOp::Mod => Value::Num(lhs % rhs),
                    BinaryOp::Pow => Value::Num(lhs.powf(rhs)),
                    BinaryOp::Less => Value::Bool(lhs < rhs),
                    BinaryOp::Great => Value::Bool(lhs > rhs),
                    _ => unreachable!(),
                }
            }
            Expr::Binary(lhs, BinaryOp::And, rhs) => {
                let lhs = self.eval(*lhs);
                if lhs.is_truthy() {
                    self.eval(*rhs)
                } else {
                    lhs
                }
            }
            Expr::Binary(lhs, BinaryOp::Or, rhs) => {
                let lhs = self.eval(*lhs);
                if lhs.is_truthy() {
                    lhs
                } else {
                    self.eval(*rhs)
                }
            }
            Expr::Binary(lhs, BinaryOp::Equal, rhs) => Value::Bool(self.eval(*lhs) == self.eval(*rhs)),
            Expr::Unary(UnaryOp::Not, expr) => Value::Bool(!self.eval(*expr).is_truthy()),
            Expr::Unary(UnaryOp::Neg, expr) => {
                let num = if let Value::Num(num) = self.eval(*expr) {
                    num
                } else {
                    panic!("invalid operand type for op {:?}", UnaryOp::Neg);
                };

                Value::Num(-num)
            }
            Expr::Call { func, args } => {
                let func = self.eval(*func);

                let args = args.into_iter().map(|expr| self.eval(expr)).collect();

                match func {
                    Value::Func(Func::Native { func, .. }) => {
                        func(self, args)
                    }
                    _ => panic!("attempt to call non-function")
                }
            }
            Expr::Lambda(_) => todo!(),
            _ => unreachable!()
        }
    }
}

#[derive(Clone)]
pub enum Func {
    User {
        name: Option<String>,
        params: Vec<String>,
        body: Box<Stmt>,
    },
    Native {
        name: String,
        func: fn(&mut Interpreter, Vec<Value>) -> Value,
    },
}

impl Debug for Func {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Func::User { name, .. } => match name {
                Some(name) => write!(f, "<func {}>", name),
                None => write!(f, "<lambda func>"),
            },
            Func::Native { name, .. } => write!(f, "<native func {}>", name),
        }
    }
}

impl PartialEq for Func {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Func::User { name, .. }, Func::User { name: name_other, .. }) => name == name_other,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Num(f64),
    String(String),
    Bool(bool),

    Vec(Rc<RefCell<Vec<Value>>>),
    Dict(Rc<RefCell<HashMap<String, Value>>>),

    Range(i32, i32),

    Func(Func),
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Num(num) => write!(f, "{}", num),
            Value::String(string) => write!(f, "{}", string),
            Value::Bool(bool) => write!(f, "{}", bool),
            Value::Vec(vec) => {
                let vec = RefCell::borrow(vec);
                write!(f, "[{}]", vec.iter().map(|v| v.to_string_quoted()).join(", "))
            }
            Value::Dict(dict) => {
                let dict = RefCell::borrow(dict);
                write!(f, "{{{}}}", dict.iter().map(|(k, v)| format!("{}: {}", k, v.to_string_quoted())).join(", "))
            }
            Value::Func(func) => write!(f, "{:?}", func),
            Value::Range(l, r) => write!(f, "{}..{}", l, r),
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(false) => false,
            _ => true,
        }
    }

    pub fn to_string_quoted(&self) -> String {
        if !matches!(self, Value::String(..)) {
            self.to_string()
        } else {
            format!("\'{}\'", self.to_string())
        }
    }
}
