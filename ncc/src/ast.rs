// TODO: we may want a const type
#[derive(Clone, Debug)]
pub enum Type
{
    Void,
    UInt(usize),
    Pointer(Box<Type>),
    Array {
        elem_type: Box<Type>,
        size_expr: Box<Expr>,
    }
}

impl Type
{
    pub fn eq(&self, other: &Type) -> bool
    {
        use Type::*;
        match (self, other) {
            (Void, Void) => true,
            (UInt(m), UInt(n)) if m == n => true,
            (Pointer(ta), Pointer(tb)) => ta.eq(tb),
            _ => false
        }
    }

    /// Produce the size of this type in bytes
    pub fn sizeof(&self) -> usize
    {
        use Type::*;
        match self {
            Void => panic!(),
            UInt(num_bits) => num_bits / 8,
            Pointer(_) => 8,
            _ => panic!()
        }
    }

    pub fn elem_type(&self) -> Type
    {
        use Type::*;
        match self {
            Pointer(t) => *t.clone(),
            _ => panic!()
        }
    }
}

/// Variable/function Declaration
#[derive(Clone, Debug)]
pub enum Decl
{
    Global { name: String, t: Type },
    Arg { idx: usize, t: Type },
    Local { idx: usize, t: Type },

    // TODO: we probably need a function signature type
    Fun { name: String },
}

impl Decl
{
    pub fn get_type(&self) -> Type
    {
        match self {
            Decl::Global { name, t } => t.clone(),
            Decl::Arg { idx, t } => t.clone(),
            Decl::Local { idx, t } => t.clone(),

            Decl::Fun { name } => todo!(),
        }
    }
}

/// Unary operator
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum UnOp
{
    Minus,
    Not,

    Deref,
    AddressOf,
}

/// Binary operator
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BinOp
{
    // Assignment
    Assign,

    And,
    Or,
    Xor,

    Add,
    Sub,
    Mul,
    Div,
    Mod,

    Eq,
    Ne,
    Lt,
    Gt,
}

/// Expression
#[derive(Clone, Debug)]
pub enum Expr
{
    Int(i128),
    String(String),

    Ident(String),

    // Reference to a variable/function declaration
    Ref(Decl),

    // TODO:
    // Type casting expression
    //Cast {
    //    t: Type,
    //    expr: Box<Expr>
    //}

    Unary {
        op: UnOp,
        child: Box<Expr>,
    },

    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    }
}

/// Statement
#[derive(Clone, Debug)]
pub enum Stmt
{
    Expr(Expr),

    ReturnExpr(Box<Expr>),
    Return,

    Break,
    Continue,

    Block(Vec<Stmt>),

    If {
        test_expr: Expr,
        then_stmt: Box<Stmt>,
        else_stmt: Option<Box<Stmt>>,
    },

    While {
        test_expr: Expr,
        body_stmt: Box<Stmt>,
    },

    For {
        init_stmt: Option<Box<Stmt>>,
        test_expr: Expr,
        incr_expr: Expr,
        body_stmt: Box<Stmt>,
    },

    /// Local variable declaration
    VarDecl {
        var_type: Type,
        var_name: String,
        init_expr: Expr,
    }
}

/// Function
#[derive(Clone, Debug)]
pub struct Function
{
    /// Name of the function
    pub name: String,

    // Return type
    pub ret_type: Type,

    /// Parameter list
    pub params: Vec<(Type, String)>,

    /// Body of the function
    pub body: Stmt,

    /// Number of local variables
    pub num_locals: usize,
}

/// Global variable declaration
#[derive(Clone, Debug)]
pub struct Global
{
    /// Name of the variable
    pub name: String,

    // Return type
    pub var_type: Type,
}

/// Top-level unit (e.g. source file)
#[derive(Default, Clone, Debug)]
pub struct Unit
{
    pub global_vars: Vec<Global>,

    pub fun_decls: Vec<Function>,
}