use {
  crate::{
    ast::Stmt,
    interp::{env::Env, Interpreter, Value},
  },
  std::{
    cell::RefCell,
    fmt::{self, Debug, Formatter},
    rc::Rc,
  },
};

#[derive(Clone)]
pub enum Func {
  User {
    name:         Option<String>,
    params:       Vec<String>,
    body:         Box<Stmt>,
    captured_env: Option<Rc<RefCell<Env>>>,
  },
  Native {
    name:     String,
    params:   Option<usize>,
    func:     fn(&mut Interpreter, Vec<Value>) -> Value,
    receiver: Option<Box<Value>>,
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
      (Func::User { name, .. }, Func::User { name: name_other, .. }) =>
        name == name_other,
      _ => false,
    }
  }
}
