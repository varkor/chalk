use crate::fallible::Fallible;
use crate::fold::Fold;
use crate::fold::shift::Shift;
use crate::ir::*;
use crate::solve::infer::InferenceTable;
use crate::solve::slg::implementation::SlgContext;
use crate::zip::{Zip, Zipper};

use chalk_slg::{ExClause, Literal};
use chalk_slg::context::{self, UnificationResult as UnificationResultTrait};
use std::sync::Arc;

///////////////////////////////////////////////////////////////////////////
// SLG RESOLVENTS
//
// The "SLG Resolvent" is used to combine a *goal* G with some
// clause or answer *C*.  It unifies the goal's selected literal
// with the clause and then inserts the clause's conditions into
// the goal's list of things to prove, basically. Although this is
// one operation in EWFS, we have specialized variants for merging
// a program clause and an answer (though they share some code in
// common).
//
// Terminology note: The NFTD and RR papers use the term
// "resolvent" to mean both the factor and the resolvent, but EWFS
// distinguishes the two. We follow EWFS here since -- in the code
// -- we tend to know whether there are delayed literals or not,
// and hence to know which code path we actually want.
//
// From EWFS:
//
// Let G be an X-clause A :- D | L1,...Ln, where N > 0, and Li be selected atom.
//
// Let C be an X-clause with no delayed literals. Let
//
//     C' = A' :- L'1...L'm
//
// be a variant of C such that G and C' have no variables in
// common.
//
// Let Li and A' be unified with MGU S.
//
// Then:
//
//     S(A :- D | L1...Li-1, L1'...L'm, Li+1...Ln)
//
// is the SLG resolvent of G with C.

impl context::ResolventOps<SlgContext> for SlgContext {
    /// Applies the SLG resolvent algorithm to incorporate a program
    /// clause into the main X-clause, producing a new X-clause that
    /// must be solved.
    ///
    /// # Parameters
    ///
    /// - `goal` is the goal G that we are trying to solve
    /// - `clause` is the program clause that may be useful to that end
    fn resolvent_clause(
        &self,
        infer: &mut InferenceTable,
        environment: &Arc<Environment>,
        goal: &DomainGoal,
        subst: &Substitution,
        clause: &ProgramClause,
    ) -> Fallible<ExClause<Self>> {
        // Relating the above description to our situation:
        //
        // - `goal` G, except with binders for any existential variables.
        //   - Also, we always select the first literal in `ex_clause.literals`, so `i` is 0.
        // - `clause` is C, except with binders for any existential variables.

        debug_heading!(
            "resolvent_clause(\
             \n    goal={:?},\
             \n    clause={:?})",
            goal,
            clause,
        );

        // C' in the description above is `consequence :- conditions`.
        //
        // Note that G and C' have no variables in common.
        let ProgramClauseImplication {
            consequence,
            conditions,
        } = infer.instantiate_binders_existentially(&clause.implication);
        debug!("consequence = {:?}", consequence);
        debug!("conditions = {:?}", conditions);

        // Unify the selected literal Li with C'.
        let unification_result = infer.unify(environment, goal, &consequence)?;

        // Final X-clause that we will return.
        let mut ex_clause = ExClause {
            subst: subst.clone(),
            delayed_literals: vec![],
            constraints: vec![],
            subgoals: vec![],
        };

        // Add the subgoals/region-constraints that unification gave us.
        unification_result.into_ex_clause(&mut ex_clause);

        // Add the `conditions` from the program clause into the result too.
        ex_clause
            .subgoals
            .extend(conditions.into_iter().map(|c| match c {
                Goal::Not(c) => Literal::Negative(InEnvironment::new(environment, *c)),
                c => Literal::Positive(InEnvironment::new(environment, c)),
            }));

        Ok(ex_clause)
    }

    ///////////////////////////////////////////////////////////////////////////
    // apply_answer_subst
    //
    // Apply answer subst has the job of "plugging in" the answer to a
    // query into the pending ex-clause. To see how it works, it's worth stepping
    // up one level. Imagine that first we are trying to prove a goal A:
    //
    //     A :- T: Foo<Vec<?U>>, ?U: Bar
    //
    // this spawns a subgoal `T: Foo<Vec<?0>>`, and it's this subgoal that
    // has now produced an answer `?0 = u32`. When the goal A spawned the
    // subgoal, it will also have registered a `PendingExClause` with its
    // current state.  At the point where *this* method has been invoked,
    // that pending ex-clause has been instantiated with fresh variables and setup,
    // so we have four bits of incoming information:
    //
    // - `ex_clause`, which is the remaining stuff to prove for the goal A.
    //   Here, the inference variable `?U` has been instantiated with a fresh variable
    //   `?X`.
    //   - `A :- ?X: Bar`
    // - `selected_goal`, which is the thing we were trying to prove when we
    //   spawned the subgoal. It shares inference variables with `ex_clause`.
    //   - `T: Foo<Vec<?X>>`
    // - `answer_table_goal`, which is the subgoal in canonical form:
    //   - `for<type> T: Foo<Vec<?0>>`
    // - `canonical_answer_subst`, which is an answer to `answer_table_goal`.
    //   - `[?0 = u32]`
    //
    // In this case, this function will (a) unify `u32` and `?X` and then
    // (b) return `ex_clause` (extended possibly with new region constraints
    // and subgoals).
    //
    // One way to do this would be to (a) substitute
    // `canonical_answer_subst` into `answer_table_goal` (yielding `T:
    // Foo<Vec<u32>>`) and then (b) instantiate the result with fresh
    // variables (no effect in this instance) and then (c) unify that with
    // `selected_goal` (yielding, indirectly, that `?X = u32`). But that
    // is not what we do: it's inefficient, to start, but it also causes
    // problems because unification of projections can make new
    // sub-goals. That is, even if the answers don't involve any
    // projections, the table goals might, and this can create an infinite
    // loop (see also #74).
    //
    // What we do instead is to (a) instantiate the substitution, which
    // may have free variables in it (in this case, it would not, and the
    // instantiation woudl have no effect) and then (b) zip
    // `answer_table_goal` and `selected_goal` without having done any
    // substitution. After all, these ought to be basically the same,
    // since `answer_table_goal` was created by canonicalizing (and
    // possibly truncating, but we'll get to that later)
    // `selected_goal`. Then, whenever we reach a "free variable" in
    // `answer_table_goal`, say `?0`, we go to the instantiated answer
    // substitution and lookup the result (in this case, `u32`). We take
    // that result and unify it with whatever we find in `selected_goal`
    // (in this case, `?X`).
    //
    // Let's cover then some corner cases. First off, what is this
    // business of instantiating the answer? Well, the answer may not be a
    // simple type like `u32`, it could be a "family" of types, like
    // `for<type> Vec<?0>` -- i.e., `Vec<X>: Bar` for *any* `X`. In that
    // case, the instantiation would produce a substitution `[?0 :=
    // Vec<?Y>]` (note that the key is not affected, just the value). So
    // when we do the unification, instead of unifying `?X = u32`, we
    // would unify `?X = Vec<?Y>`.
    //
    // Next, truncation. One key thing is that the `answer_table_goal` may
    // not be *exactly* the same as the `selected_goal` -- we will
    // truncate it if it gets too deep. so, in our example, it may be that
    // instead of `answer_table_goal` being `for<type> T: Foo<Vec<?0>>`,
    // it could have been truncated to `for<type> T: Foo<?0>` (which is a
    // more general goal).  In that case, let's say that the answer is
    // still `[?0 = u32]`, meaning that `T: Foo<u32>` is true (which isn't
    // actually interesting to our original goal). When we do the zip
    // then, we will encounter `?0` in the `answer_table_goal` and pair
    // that with `Vec<?X>` from the pending goal. We will attempt to unify
    // `Vec<?X>` with `u32` (from the substitution), which will fail. That
    // failure will get propagated back up.

    fn apply_answer_subst(
        &self,
        infer: &mut InferenceTable,
        ex_clause: ExClause<SlgContext>,
        selected_goal: &InEnvironment<Goal>,
        answer_table_goal: &Canonical<InEnvironment<Goal>>,
        canonical_answer_subst: &Canonical<ConstrainedSubst>,
    ) -> Fallible<ExClause<SlgContext>> {
        debug_heading!("apply_answer_subst()");
        debug!("ex_clause={:?}", ex_clause);
        debug!("selected_goal={:?}", infer.normalize_deep(selected_goal));
        debug!("answer_table_goal={:?}", answer_table_goal);
        debug!("canonical_answer_subst={:?}", canonical_answer_subst);

        // C' is now `answer`. No variables in commmon with G.
        let ConstrainedSubst {
            subst: answer_subst,

            // Assuming unification succeeds, we incorporate the
            // region constraints from the answer into the result;
            // we'll need them if this answer (which is not yet known
            // to be true) winds up being true, and otherwise (if the
            // answer is false or unknown) it doesn't matter.
            constraints: answer_constraints,
        } = infer.instantiate_canonical(&canonical_answer_subst);

        let mut ex_clause = AnswerSubstitutor::substitute(
            infer,
            &selected_goal.environment,
            &answer_subst,
            ex_clause,
            &answer_table_goal.value,
            selected_goal,
        )?;
        ex_clause.constraints.extend(answer_constraints);
        Ok(ex_clause)
    }
}

struct AnswerSubstitutor<'t> {
    table: &'t mut InferenceTable,
    environment: &'t Arc<Environment>,
    answer_subst: &'t Substitution,
    answer_binders: usize,
    pending_binders: usize,
    ex_clause: ExClause<SlgContext>,
}

impl<'t> AnswerSubstitutor<'t> {
    fn substitute<T: Zip>(
        table: &mut InferenceTable,
        environment: &Arc<Environment>,
        answer_subst: &Substitution,
        ex_clause: ExClause<SlgContext>,
        answer: &T,
        pending: &T,
    ) -> Fallible<ExClause<SlgContext>> {
        let mut this = AnswerSubstitutor {
            table,
            environment,
            answer_subst,
            ex_clause,
            answer_binders: 0,
            pending_binders: 0,
        };
        Zip::zip_with(&mut this, answer, pending)?;
        Ok(this.ex_clause)
    }

    fn unify_free_answer_var(
        &mut self,
        answer_depth: usize,
        pending: ParameterKind<&Ty, &Lifetime, &Const>,
    ) -> Fallible<bool> {
        // This variable is bound in the answer, not free, so it
        // doesn't represent a reference into the answer substitution.
        if answer_depth < self.answer_binders {
            return Ok(false);
        }

        let answer_param = &self.answer_subst.parameters[answer_depth - self.answer_binders];

        let pending_shifted = &pending
            .down_shift(self.pending_binders)
            .unwrap_or_else(|_| {
                panic!(
                    "truncate extracted a pending value that references internal binder: {:?}",
                    pending,
                )
            });

        self.table
            .unify(&self.environment, answer_param, pending_shifted)?
            .into_ex_clause(&mut self.ex_clause);

        Ok(true)
    }

    /// When we encounter a variable in the answer goal, we first try
    /// `unify_free_answer_var`. Assuming that this fails, the
    /// variable must be a bound variable in the answer goal -- in
    /// that case, there should be a corresponding bound variable in
    /// the pending goal. This bit of code just checks that latter
    /// case.
    fn assert_matching_vars(&mut self, answer_depth: usize, pending_depth: usize) -> Fallible<()> {
        assert!(answer_depth < self.answer_binders);
        assert!(pending_depth < self.answer_binders);
        assert_eq!(
            answer_depth - self.answer_binders,
            pending_depth - self.pending_binders
        );
        Ok(())
    }
}

impl<'t> Zipper for AnswerSubstitutor<'t> {
    fn zip_tys(&mut self, answer: &Ty, pending: &Ty) -> Fallible<()> {
        if let Some(pending) = self.table.normalize_shallow(pending, self.pending_binders) {
            return Zip::zip_with(self, answer, &pending);
        }

        // If the answer has a variable here, then this is one of the
        // "inputs" to the subgoal table. We need to extract the
        // resulting answer that the subgoal found and unify it with
        // the value from our "pending subgoal".
        if let Ty::Var(answer_depth) = answer {
            if self.unify_free_answer_var(*answer_depth, ParameterKind::Ty(pending))? {
                return Ok(());
            }
        }

        // Otherwise, the answer and the selected subgoal ought to be a perfect match for
        // one another.
        match (answer, pending) {
            (Ty::Var(answer_depth), Ty::Var(pending_depth)) => {
                self.assert_matching_vars(*answer_depth, *pending_depth)
            }

            (Ty::Apply(answer), Ty::Apply(pending)) => Zip::zip_with(self, answer, pending),

            (Ty::Projection(answer), Ty::Projection(pending)) => {
                Zip::zip_with(self, answer, pending)
            }

            (Ty::UnselectedProjection(answer), Ty::UnselectedProjection(pending)) => {
                Zip::zip_with(self, answer, pending)
            }

            (Ty::ForAll(answer), Ty::ForAll(pending)) => {
                self.answer_binders += answer.num_binders;
                self.pending_binders += pending.num_binders;
                Zip::zip_with(self, &answer.ty, &pending.ty)?;
                self.answer_binders -= answer.num_binders;
                self.pending_binders -= pending.num_binders;
                Ok(())
            }

            (Ty::Var(_), _)
            | (Ty::Apply(_), _)
            | (Ty::Projection(_), _)
            | (Ty::UnselectedProjection(_), _)
            | (Ty::ForAll(_), _) => panic!(
                "structural mismatch between answer `{:?}` and pending goal `{:?}`",
                answer, pending,
            ),
        }
    }

    fn zip_lifetimes(&mut self, answer: &Lifetime, pending: &Lifetime) -> Fallible<()> {
        if let Some(pending) = self.table.normalize_lifetime(pending, self.pending_binders) {
            return Zip::zip_with(self, answer, &pending);
        }

        if let Lifetime::Var(answer_depth) = answer {
            if self.unify_free_answer_var(*answer_depth, ParameterKind::Lifetime(pending))? {
                return Ok(());
            }
        }

        match (answer, pending) {
            (Lifetime::Var(answer_depth), Lifetime::Var(pending_depth)) => {
                self.assert_matching_vars(*answer_depth, *pending_depth)
            }

            (Lifetime::ForAll(answer_ui), Lifetime::ForAll(pending_ui)) => {
                assert_eq!(answer_ui, pending_ui);
                Ok(())
            }

            (Lifetime::Var(_), _) | (Lifetime::ForAll(_), _) => panic!(
                "structural mismatch between answer `{:?}` and pending goal `{:?}`",
                answer, pending,
            ),
        }
    }

    fn zip_consts(&mut self, answer: &Const, pending: &Const) -> Fallible<()> {
        if let Some(pending) = self.table.normalize_const(pending, self.pending_binders) {
            return Zip::zip_with(self, answer, &pending);
        }

        let Const::Var(answer_depth) = answer;
        if self.unify_free_answer_var(*answer_depth, ParameterKind::Const(pending))? {
            return Ok(());
        }

        match (answer, pending) {
            (Const::Var(answer_depth), Const::Var(pending_depth)) => {
                self.assert_matching_vars(*answer_depth, *pending_depth)
            }
        }
    }

    fn zip_binders<T>(&mut self, answer: &Binders<T>, pending: &Binders<T>) -> Fallible<()>
    where
        T: Zip + Fold<Result = T>,
    {
        self.answer_binders += answer.binders.len();
        self.pending_binders += pending.binders.len();
        Zip::zip_with(self, &answer.value, &pending.value)?;
        self.answer_binders -= answer.binders.len();
        self.pending_binders -= pending.binders.len();
        Ok(())
    }
}
