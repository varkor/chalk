use std::fmt::{Debug, Display, Error, Formatter};

use super::*;

impl Debug for ItemId {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        tls::with_current_program(|p| match p {
            Some(prog) => if let Some(k) = prog.type_kinds.get(self) {
                write!(fmt, "{}", k.name)
            } else if let Some(k) = prog.associated_ty_data.get(self) {
                write!(fmt, "({:?}::{})", k.trait_id, k.name)
            } else {
                fmt.debug_struct("ItemId")
                    .field("index", &self.index)
                    .finish()
            },
            None => fmt.debug_struct("ItemId")
                .field("index", &self.index)
                .finish(),
        })
    }
}

impl Display for UniverseIndex {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "U{}", self.counter)
    }
}

impl Debug for UniverseIndex {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "U{}", self.counter)
    }
}

impl Debug for TypeName {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            TypeName::ItemId(id) => write!(fmt, "{:?}", id),
            TypeName::ForAll(universe) => write!(fmt, "!{}", universe.counter),
            TypeName::AssociatedType(assoc_ty) => write!(fmt, "{:?}", assoc_ty),
        }
    }
}

impl Debug for Ty {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            Ty::Var(depth) => write!(fmt, "?{}", depth),
            Ty::Apply(ref apply) => write!(fmt, "{:?}", apply),
            Ty::Projection(ref proj) => write!(fmt, "{:?}", proj),
            Ty::UnselectedProjection(ref proj) => write!(fmt, "{:?}", proj),
            Ty::ForAll(ref quantified_ty) => write!(fmt, "{:?}", quantified_ty),
        }
    }
}

impl Debug for QuantifiedTy {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        // FIXME -- we should introduce some names or something here
        let QuantifiedTy {
            num_binders,
            ref ty,
        } = *self;
        write!(fmt, "for<{}> {:?}", num_binders, ty)
    }
}

impl Debug for Lifetime {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            Lifetime::Var(depth) => write!(fmt, "'?{}", depth),
            Lifetime::ForAll(universe) => write!(fmt, "'!{}", universe.counter),
        }
    }
}

impl Debug for Const {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            Const::Var(depth) => write!(fmt, "const ?{}", depth),
        }
    }
}

impl Debug for ApplicationTy {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "{:?}{:?}", self.name, Angle(&self.parameters))
    }
}

impl Debug for TraitRef {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(
            fmt,
            "{:?} as {:?}{:?}",
            self.parameters[0],
            self.trait_id,
            Angle(&self.parameters[1..])
        )
    }
}

impl Debug for ProjectionTy {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        tls::with_current_program(|p| match p {
            Some(program) => {
                let (associated_ty_data, trait_params, other_params) =
                    program.split_projection(self);
                write!(
                    fmt,
                    "<{:?} as {:?}{:?}>::{}{:?}",
                    &trait_params[0],
                    associated_ty_data.trait_id,
                    Angle(&trait_params[1..]),
                    associated_ty_data.name,
                    Angle(&other_params)
                )
            }
            None => write!(
                fmt,
                "({:?}){:?}",
                self.associated_ty_id,
                Angle(&self.parameters)
            ),
        })
    }
}

impl Debug for UnselectedProjectionTy {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        let len = self.parameters.len();
        write!(
            fmt,
            "{:?}::{}{:?}",
            self.parameters[len - 1],
            self.type_name,
            Angle(&self.parameters[0..(len - 1)])
        )
    }
}

crate struct Angle<'a, T: 'a>(pub &'a [T]);

impl<'a, T: Debug> Debug for Angle<'a, T> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        if self.0.len() > 0 {
            write!(fmt, "<")?;
            for (index, elem) in self.0.iter().enumerate() {
                if index > 0 {
                    write!(fmt, ", {:?}", elem)?;
                } else {
                    write!(fmt, "{:?}", elem)?;
                }
            }
            write!(fmt, ">")?;
        }
        Ok(())
    }
}

impl Debug for Normalize {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "Normalize({:?} -> {:?})", self.projection, self.ty)
    }
}

impl Debug for ProjectionEq {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "ProjectionEq({:?} = {:?})", self.projection, self.ty)
    }
}

impl Debug for UnselectedNormalize {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "UnselectedNormalize({:?} -> {:?})", self.projection, self.ty)
    }
}

impl Debug for DomainGoal {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            DomainGoal::Normalize(ref n) => write!(fmt, "{:?}", n),
            DomainGoal::ProjectionEq(ref n) => write!(fmt, "{:?}", n),
            DomainGoal::UnselectedNormalize(ref n) => write!(fmt, "{:?}", n),
            DomainGoal::Implemented(ref n) => write!(
                fmt,
                "({:?}: {:?}{:?})",
                n.parameters[0],
                n.trait_id,
                Angle(&n.parameters[1..])
            ),
            DomainGoal::WellFormed(ref n) => write!(fmt, "{:?}", n),
            DomainGoal::FromEnv(ref n) => write!(fmt, "{:?}", n),
            DomainGoal::InScope(ref n) => write!(fmt, "InScope({:?})", n),
        }
    }
}

impl Debug for LeafGoal {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            LeafGoal::EqGoal(ref eq) => write!(fmt, "{:?}", eq),
            LeafGoal::DomainGoal(ref dom) => write!(fmt, "{:?}", dom),
        }
    }
}

impl Debug for WellFormed {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        let value: &dyn Debug = match *self {
            WellFormed::Ty(ref t) => t,
            WellFormed::TraitRef(ref t) => t,
            WellFormed::ProjectionEq(ref t) => t,
        };
        write!(fmt, "WellFormed({:?})", value)
    }
}

impl Debug for FromEnv {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        let value: &dyn Debug = match *self {
            FromEnv::Ty(ref t) => t,
            FromEnv::TraitRef(ref t) => t,
            FromEnv::ProjectionEq(ref t) => t,
        };
        write!(fmt, "FromEnv({:?})", value)
    }
}

impl Debug for EqGoal {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "({:?} = {:?})", self.a, self.b)
    }
}

impl Debug for Goal {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            Goal::Quantified(qkind, ref subgoal) => {
                write!(fmt, "{:?}<", qkind)?;
                for (index, binder) in subgoal.binders.iter().enumerate() {
                    if index > 0 {
                        write!(fmt, ", ")?;
                    }
                    match *binder {
                        ParameterKind::Ty(()) => write!(fmt, "type")?,
                        ParameterKind::Lifetime(()) => write!(fmt, "lifetime")?,
                        ParameterKind::Const(()) => write!(fmt, "const")?,
                    }
                }
                write!(fmt, "> {{ {:?} }}", subgoal.value)
            }
            Goal::Implies(ref wc, ref g) => write!(fmt, "if ({:?}) {{ {:?} }}", wc, g),
            Goal::And(ref g1, ref g2) => write!(fmt, "({:?}, {:?})", g1, g2),
            Goal::Not(ref g) => write!(fmt, "not {{ {:?} }}", g),
            Goal::Leaf(ref wc) => write!(fmt, "{:?}", wc),
            Goal::CannotProve(()) => write!(fmt, r"¯\_(ツ)_/¯"),
        }
    }
}

impl<T: Debug> Debug for Binders<T> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        let Binders {
            ref binders,
            ref value,
        } = *self;
        if !binders.is_empty() {
            write!(fmt, "for<")?;
            for (index, binder) in binders.iter().enumerate() {
                if index > 0 {
                    write!(fmt, ", ")?;
                }
                match *binder {
                    ParameterKind::Ty(()) => write!(fmt, "type")?,
                    ParameterKind::Lifetime(()) => write!(fmt, "lifetime")?,
                    ParameterKind::Const(()) => write!(fmt, "const")?,
                }
            }
            write!(fmt, "> ")?;
        }
        Debug::fmt(value, fmt)
    }
}

impl Debug for Environment {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "Env({:?})", self.clauses)
    }
}

impl<T: Display> Display for Canonical<T> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let Canonical { binders, value } = self;

        if binders.is_empty() {
            write!(f, "{}", value)?;
        } else {
            write!(f, "for<")?;

            for (i, pk) in binders.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "?{}", pk.into_inner())?;
            }

            write!(f, "> {{ {} }}", value)?;
        }

        Ok(())
    }
}

impl<T: Debug, L: Debug, C: Debug> Debug for ParameterKind<T, L, C> {
    default fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            ParameterKind::Ty(ref n) => write!(fmt, "Ty({:?})", n),
            ParameterKind::Lifetime(ref n) => write!(fmt, "Lifetime({:?})", n),
            ParameterKind::Const(ref n) => write!(fmt, "Const({:?})", n),
        }
    }
}

impl Debug for Constraint {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match self {
            Constraint::LifetimeEq(a, b) => write!(fmt, "{:?} == {:?}", a, b),
        }
    }
}

impl Debug for Parameter {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        match *self {
            ParameterKind::Ty(ref n) => write!(fmt, "{:?}", n),
            ParameterKind::Lifetime(ref n) => write!(fmt, "{:?}", n),
            ParameterKind::Const(ref n) => write!(fmt, "{:?}", n),
        }
    }
}

impl Display for ConstrainedSubst {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let ConstrainedSubst { subst, constraints } = self;

        write!(
            f,
            "substitution {}, lifetime constraints {:?}",
            subst,
            constraints,
        )
    }
}

impl Debug for Substitution {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        Display::fmt(self, f)
    }
}

impl Display for Substitution {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let mut first = true;

        write!(f, "[")?;

        for (index, value) in self.parameters.iter().enumerate() {
            if first {
                first = false;
            } else {
                write!(f, ", ")?;
            }

            write!(f, "?{} := {:?}", index, value)?;
        }

        write!(f, "]")?;

        Ok(())
    }
}
