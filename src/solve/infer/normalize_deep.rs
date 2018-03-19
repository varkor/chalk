use fallible::*;
use fold::{DefaultTypeFolder, ExistentialFolder, Fold, IdentityUniversalFolder};
use fold::shift::Shift;
use ir::*;

use super::{InferenceTable, InferenceVariable};

impl InferenceTable {
    /// Given a value `value` with variables in it, replaces those variables
    /// with their instantiated values (if any). Uninstantiated variables are
    /// left as-is.
    ///
    /// This is mainly intended for getting final values to dump to
    /// the user and its use should otherwise be avoided, particularly
    /// given the possibility of snapshots and rollbacks.
    ///
    /// See also `InferenceTable::canonicalize`, which -- during real
    /// processing -- is often used to capture the "current state" of
    /// variables.
    crate fn normalize_deep<T: Fold>(&mut self, value: &T) -> T::Result {
        value
            .fold_with(&mut DeepNormalizer { table: self }, 0)
            .unwrap()
    }
}

struct DeepNormalizer<'table> {
    table: &'table mut InferenceTable,
}

impl<'table> DefaultTypeFolder for DeepNormalizer<'table> {}

impl<'table> IdentityUniversalFolder for DeepNormalizer<'table> {}

impl<'table> ExistentialFolder for DeepNormalizer<'table> {
    fn fold_free_existential_ty(&mut self, depth: usize, binders: usize) -> Fallible<Ty> {
        let var = InferenceVariable::from_depth(depth);
        match self.table.probe_ty_var(var) {
            Some(ty) => Ok(ty.fold_with(self, 0)?.up_shift(binders)),
            None => Ok(InferenceVariable::from_depth(depth + binders).to_ty()),
        }
    }

    fn fold_free_existential_lifetime(
        &mut self,
        depth: usize,
        binders: usize,
    ) -> Fallible<Lifetime> {
        let var = InferenceVariable::from_depth(depth);
        match self.table.probe_lifetime_var(var) {
            Some(l) => Ok(l.fold_with(self, 0)?.up_shift(binders)),
            None => Ok(InferenceVariable::from_depth(depth + binders).to_lifetime()),
        }
    }

    fn fold_free_existential_const(
        &mut self,
        depth: usize,
        binders: usize,
    ) -> Fallible<Const> {
        let var = InferenceVariable::from_depth(depth);
        match self.table.probe_const_var(var) {
            Some(l) => Ok(l.fold_with(self, 0)?.up_shift(binders)),
            None => Ok(InferenceVariable::from_depth(depth + binders).to_const()),
        }
    }
}
