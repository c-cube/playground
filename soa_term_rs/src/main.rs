use {
    anyhow::Result,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, ops::Index},
};

#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
enum Kind {
    App,
    Lam,
    Type,
    Var,
    DBVar,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Var(u32);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Term(Kind, u32);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum View {
    App(Term, Term),
    Lam(Term, Term),
    Var(Var),
    DBVar(u16),
    Type(u8),
}

/// Term manager
#[derive(Default, Serialize, Deserialize)]
pub struct TermStore {
    hcons: HashMap<View, Term>,
    hcons_var: HashMap<String, Var>,
    apps: Vec<(Term, Term)>,
    lambdas: Vec<(Term, Term)>,
    vars: Vec<String>,
}

impl TermStore {
    pub fn new() -> Self {
        Default::default()
    }

    /// Number of composite terms (application, lambda…)
    pub fn n_composite_terms(&self) -> usize {
        self.hcons.len()
    }

    fn hcons_var_(&mut self, v: &str) -> Var {
        match self.hcons_var.get(v) {
            Some(v) => *v,
            None => {
                let i = self.vars.len() as u32;
                let name: String = v.into();
                self.vars.push(name.clone());
                self.hcons_var.insert(name, Var(i));
                Var(i)
            }
        }
    }

    fn hcons_(&mut self, v: View) -> Term {
        match v {
            View::Var(var) => {
                // no hashconsing for variables, `v` is already hashconsed
                Term(Kind::Var, var.0)
            }
            View::DBVar(var) => Term(Kind::DBVar, var as u32),
            View::Type(lvl) => {
                // no hashconsing for type, level is enough
                Term(Kind::Type, lvl as u32)
            }
            View::App(f, a) => match self.hcons.get(&v) {
                Some(t) => *t,
                None => {
                    let i = self.apps.len() as u32;
                    self.apps.push((f, a));
                    let t = Term(Kind::App, i);
                    self.hcons.insert(v, t);
                    t
                }
            },
            View::Lam(tyv, bod) => match self.hcons.get(&v) {
                Some(t) => *t,
                None => {
                    let i = self.lambdas.len() as u32;
                    self.lambdas.push((tyv, bod));
                    let t = Term(Kind::Lam, i);
                    self.hcons.insert(v, t);
                    t
                }
            },
        }
    }

    pub fn var(&mut self, v: &str) -> Term {
        let v = self.hcons_var_(v);
        self.hcons_(View::Var(v))
    }

    pub fn db_var(&mut self, idx: u16) -> Term {
        self.hcons_(View::DBVar(idx))
    }

    pub fn app(&mut self, f: Term, a: Term) -> Term {
        self.hcons_(View::App(f, a))
    }

    pub fn app_l(&mut self, f: Term, args: &[Term]) -> Term {
        let mut t = f;
        for a in args {
            t = self.hcons_(View::App(t, *a))
        }
        t
    }

    pub fn lam(&mut self, tyv: Term, bod: Term) -> Term {
        self.hcons_(View::Lam(tyv, bod))
    }

    pub fn type_(&mut self, lvl: u8) -> Term {
        self.hcons_(View::Type(lvl))
    }

    fn print_into_(&self, s: &mut String, t: Term) {
        match self.view(t) {
            View::App(f, a) => {
                *s += "(";
                self.print_into_(s, f);
                *s += " ";
                self.print_into_(s, a);
                *s += ")";
            }
            View::Lam(tyv, bod) => {
                *s += "(\\0:";
                self.print_into_(s, tyv);
                *s += " -> ";
                self.print_into_(s, bod);
                *s += ")";
            }
            View::Var(v) => *s += &self[v],
            View::DBVar(v) => {
                *s += &format!("db{}", v);
            }
            View::Type(_) => *s += "type",
        }
    }

    /// Print term
    pub fn print(&self, t: Term) -> String {
        let mut s = String::new();
        self.print_into_(&mut s, t);
        s
    }

    /// Access the given term.
    #[inline]
    pub fn view(&self, t: Term) -> View {
        match t.0 {
            Kind::App => {
                let a = self.apps[t.1 as usize];
                View::App(a.0, a.1)
            }
            Kind::Lam => {
                let a = self.lambdas[t.1 as usize];
                View::Lam(a.0, a.1)
            }
            Kind::Type => View::Type(t.1 as u8),
            Kind::Var => View::Var(Var(t.1)),
            Kind::DBVar => View::DBVar(t.1 as u16),
        }
    }
}

impl Index<Var> for TermStore {
    type Output = str;

    fn index(&self, v: Var) -> &Self::Output {
        &self.vars[v.0 as usize]
    }
}

pub fn main() -> Result<()> {
    let mut tst = TermStore::new();
    let ty = tst.type_(0);
    let f = tst.var("f");
    let a = tst.var("a");
    let v = tst.app(f, a);
    dbg!(tst.print(v));
    dbg!(std::mem::size_of::<Term>());

    let big_term = {
        let db0 = tst.db_var(0);
        let mut t = tst.app_l(f, &[a, db0]);
        for _i in 0..100 {
            t = tst.app(f, t)
        }
        t = tst.lam(ty, t);
        t
    };

    dbg!(tst.print(big_term));
    dbg!(tst.n_composite_terms());

    let store = {
        let mut buf: Vec<u8> = Vec::new();
        serde_cbor::to_writer(&mut buf, &tst)?;
        buf
    };
    println!("store encoded to cbor: {} Bytes", store.len());

    let tst2: TermStore = serde_cbor::from_reader(&store[..])?;
    dbg!(tst2.print(v));
    dbg!(tst2.print(big_term));

    Ok(())
}
