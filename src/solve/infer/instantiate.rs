use fallible::*;
use fold::*;
use std::fmt::Debug;

use super::*;

impl InferenceTable {
    /// Create a instance of `arg` where each variable is replaced with
    /// a fresh inference variable of suitable kind.
    fn instantiate<U, T>(&mut self, universes: U, arg: &T) -> T::Result
    where
        T: Fold + Debug,
        U: IntoIterator<Item = ParameterKind<UniverseIndex>>,
    {
        debug!("instantiate(arg={:?})", arg);
        let vars: Vec<_> = universes
            .into_iter()
            .map(|param_kind| self.parameter_kind_to_parameter(param_kind))
            .collect();
        debug!("instantiate: vars={:?}", vars);
        let mut instantiator = Instantiator { vars };
        arg.fold_with(&mut instantiator, 0).expect("")
    }

    fn parameter_kind_to_parameter(
        &mut self,
        param_kind: ParameterKind<UniverseIndex>,
    ) -> Parameter {
        match param_kind {
            ParameterKind::Ty(ui) => ParameterKind::Ty(self.new_variable(ui).to_ty()),
            ParameterKind::Lifetime(ui) => {
                ParameterKind::Lifetime(self.new_variable(ui).to_lifetime())
            }
            ParameterKind::Const(ui) => {
                ParameterKind::Const(self.new_variable(ui).to_const())
            }
        }
    }

    /// Given the binders from a canonicalized value C, returns a
    /// substitution S mapping each free variable in C to a fresh
    /// inference variable. This substitution can then be applied to
    /// C, which would be equivalent to
    /// `self.instantiate_canonical(v)`.
    crate fn fresh_subst(&mut self, binders: &[ParameterKind<UniverseIndex>]) -> Substitution {
        Substitution {
            parameters: binders
                .iter()
                .map(|kind| {
                    let param_infer_var = kind.map(|ui| self.new_variable(ui));
                    param_infer_var.to_parameter()
                })
                .collect(),
        }
    }

    /// Variant on `instantiate` that takes a `Canonical<T>`.
    crate fn instantiate_canonical<T>(&mut self, bound: &Canonical<T>) -> T::Result
    where
        T: Fold + Debug,
    {
        self.instantiate(bound.binders.iter().cloned(), &bound.value)
    }

    /// Instantiates `arg` with fresh existential variables in the
    /// given universe; the kinds of the variables are implied by
    /// `binders`. This is used to apply a universally quantified
    /// clause like `forall X, 'Y. P => Q`. Here the `binders`
    /// argument is referring to `X, 'Y`.
    crate fn instantiate_in<U, T>(
        &mut self,
        universe: UniverseIndex,
        binders: U,
        arg: &T,
    ) -> T::Result
    where
        T: Fold,
        U: IntoIterator<Item = ParameterKind<()>>,
    {
        self.instantiate(binders.into_iter().map(|pk| pk.map(|_| universe)), arg)
    }

    /// Variant on `instantiate_in` that takes a `Binders<T>`.
    #[allow(non_camel_case_types)]
    crate fn instantiate_binders_existentially<T>(
        &mut self,
        arg: &impl BindersAndValue<Output = T>,
    ) -> T::Result
    where
        T: Fold,
    {
        let (binders, value) = arg.split();
        let max_universe = self.max_universe;
        self.instantiate_in(max_universe, binders.iter().cloned(), value)
    }

    #[allow(non_camel_case_types)]
    crate fn instantiate_binders_universally<T>(
        &mut self,
        arg: &impl BindersAndValue<Output = T>,
    ) -> T::Result
    where
        T: Fold,
    {
        let (binders, value) = arg.split();
        let parameters: Vec<_> = binders
            .iter()
            .map(|pk| {
                let new_universe = self.new_universe();
                match *pk {
                    ParameterKind::Lifetime(()) => {
                        let lt = Lifetime::ForAll(new_universe);
                        ParameterKind::Lifetime(lt)
                    }
                    ParameterKind::Const(()) => unimplemented!(), // TODO(varkor)
                    ParameterKind::Ty(()) => ParameterKind::Ty(Ty::Apply(ApplicationTy {
                        name: TypeName::ForAll(new_universe),
                        parameters: vec![],
                    })),
                }
            })
            .collect();
        Subst::apply(&parameters, value)
    }
}

crate trait BindersAndValue {
    type Output;

    fn split(&self) -> (&[ParameterKind<()>], &Self::Output);
}

impl<T> BindersAndValue for Binders<T> {
    type Output = T;

    fn split(&self) -> (&[ParameterKind<()>], &Self::Output) {
        (&self.binders, &self.value)
    }
}

impl<'a, T> BindersAndValue for (&'a Vec<ParameterKind<()>>, &'a T) {
    type Output = T;

    fn split(&self) -> (&[ParameterKind<()>], &Self::Output) {
        (&self.0, &self.1)
    }
}

struct Instantiator {
    vars: Vec<Parameter>,
}

impl DefaultTypeFolder for Instantiator {}

/// When we encounter a free variable (of any kind) with index
/// `i`, we want to map anything in the first N binders to
/// `self.vars[i]`. Everything else stays intact, but we have to
/// subtract `self.vars.len()` to account for the binders we are
/// instantiating.
impl ExistentialFolder for Instantiator {
    fn fold_free_existential_ty(&mut self, depth: usize, binders: usize) -> Fallible<Ty> {
        if depth < self.vars.len() {
            Ok(self.vars[depth].assert_ty_ref().up_shift(binders))
        } else {
            Ok(Ty::Var(depth + binders - self.vars.len())) // see comment above
        }
    }

    fn fold_free_existential_lifetime(
        &mut self,
        depth: usize,
        binders: usize,
    ) -> Fallible<Lifetime> {
        if depth < self.vars.len() {
            Ok(self.vars[depth].assert_lifetime_ref().up_shift(binders))
        } else {
            Ok(Lifetime::Var(depth + binders - self.vars.len())) // see comment above
        }
    }

    fn fold_free_existential_const(
        &mut self,
        depth: usize,
        binders: usize,
    ) -> Fallible<Const> {
        if depth < self.vars.len() {
            Ok(self.vars[depth].assert_const_ref().up_shift(binders))
        } else {
            Ok(Const::Var(depth + binders - self.vars.len())) // see comment above
        }
    }
}

impl IdentityUniversalFolder for Instantiator {}
