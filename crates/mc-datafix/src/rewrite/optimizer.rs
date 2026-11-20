//! `DataFixerUpper.OPTIMIZATION_RULE`:
//!
//! ```text
//! everywhere(
//!     seq(CataFuseSame, CataFuseDifferent, together(LensComp, SortProj, SortInj)),
//!     AppNest)
//! ```
//!
//! `everywhere(topDown, bottomUp)` applies `topDown` to a node, then the whole rule to its children (`PointFree.all`),
//! then `bottomUp`. The three composition rules (`CompRewrite`) work on the functions of a `Comp`: each pass walks them
//! from the one applied last to the one applied first and replaces an adjacent pair by the rule's rewrite. Nothing is
//! iterated to a fixed point. A rule that changes nothing returns the very node it was given, which is how a parent
//! knows a child changed (`==` in Java, [`PointFree::ptr_eq`] here).
//!
//! Only the node kinds the rule matches are modelled: an `Apply` is always `Apply(ProfunctorTransformer(optic), f)`
//! (`PfKind::App`), the one place Java builds an `Apply`.

use super::{BitSet, FoldShared, PfKind, PointFree, RewriteResult, View};
use crate::optic::{Optic, TypedOptic};
use crate::ty::Type;
use std::collections::VecDeque;
use std::sync::Arc;

/// `CompRewrite.doRewrite(first, second)`: `first` is applied after `second`.
type CompRule = fn(&PointFree, &PointFree) -> Option<PointFree>;

const COMP_RULES: [CompRule; 3] = [cata_fuse_same, cata_fuse_different, together];

/// `OPTIMIZATION_RULE.rewriteOrNop(expr)` (the rule always yields a result).
pub(super) fn optimize(expr: &PointFree) -> PointFree {
    let mut top = expr.clone();
    for rule in COMP_RULES {
        if let Some(rewritten) = rewrite_comp(&top, rule) {
            top = rewritten;
        }
    }
    let all = all(&top).unwrap_or(top);
    app_nest(&all).unwrap_or(all)
}

/// `expr.all(OPTIMIZATION_RULE)`: `None` when no child changed.
fn all(expr: &PointFree) -> Option<PointFree> {
    let (ty, new_ty) = (expr.0.ty.clone(), expr.0.new_ty.clone());
    match &expr.0.kind {
        PfKind::Comp(functions) => {
            let mut rewritten = false;
            let mut new_functions = Vec::with_capacity(functions.len());
            for function in functions {
                let rewrite = optimize(function);
                if rewrite.ptr_eq(function) {
                    new_functions.push(function.clone());
                    continue;
                }
                rewritten = true;
                match comp_functions(&rewrite) {
                    Some(nested) => new_functions.extend(nested.iter().cloned()),
                    None => new_functions.push(rewrite),
                }
            }
            rewritten.then(|| PointFree::new(PfKind::Comp(new_functions), ty, new_ty))
        }
        PfKind::App { optic, f } => {
            let arg = optimize(f);
            (!arg.ptr_eq(f)).then(|| {
                PointFree::new(
                    PfKind::App {
                        optic: optic.clone(),
                        f: arg,
                    },
                    ty,
                    new_ty,
                )
            })
        }
        PfKind::Fold { shared, index } => {
            let mut changed = false;
            let algebra: Vec<RewriteResult> = shared
                .algebra
                .iter()
                .map(|result| {
                    let rewrite = optimize(&result.view.0);
                    if rewrite.ptr_eq(&result.view.0) {
                        return result.clone();
                    }
                    changed = true;
                    RewriteResult::create(View(rewrite), result.rec_data)
                })
                .collect();
            changed.then(|| fold(shared, algebra, *index))
        }
        PfKind::Id | PfKind::In | PfKind::Fun { .. } | PfKind::TaggedChoice { .. } => None,
    }
}

fn comp_functions(expr: &PointFree) -> Option<&[PointFree]> {
    match &expr.0.kind {
        PfKind::Comp(functions) => Some(functions),
        _ => None,
    }
}

/// `family.fold(new ListAlgebra(..), newFamily).apply(index).view().function()` over the families of `shared`.
fn fold(shared: &FoldShared, algebra: Vec<RewriteResult>, index: i32) -> PointFree {
    FoldShared::fold_fn(&shared.family, Arc::new(algebra), &shared.new_family)(index)
        .view
        .0
}

/// `CompRewrite.rewrite(expr)`.
fn rewrite_comp(expr: &PointFree, rule: CompRule) -> Option<PointFree> {
    let mut queue: VecDeque<PointFree> = comp_functions(expr)?.iter().cloned().collect();
    let mut result: Vec<PointFree> = Vec::with_capacity(queue.len());
    let mut rewritten = false;
    while let Some(next) = queue.pop_front() {
        match result.last().and_then(|last| rule(last, &next)) {
            Some(rewrite) => {
                result.pop();
                match comp_functions(&rewrite) {
                    Some(nested) => nested
                        .iter()
                        .rev()
                        .for_each(|f| queue.push_front(f.clone())),
                    None => queue.push_front(rewrite),
                }
                rewritten = true;
            }
            None => result.push(next),
        }
    }
    if !rewritten {
        return None;
    }
    Some(if result.len() == 1 {
        result.remove(0)
    } else {
        PointFree::comp_raw(result)
    })
}

/// `CompRewrite.together(LensComp, SortProj, SortInj)`.
fn together(first: &PointFree, second: &PointFree) -> Option<PointFree> {
    lens_comp(first, second)
        .or_else(|| sort_proj(first, second))
        .or_else(|| sort_inj(first, second))
}

/// The two folds of one family at one index (`CataFuse*` precondition).
fn same_family_folds<'a>(
    first: &'a PointFree,
    second: &'a PointFree,
) -> Option<(&'a FoldShared, &'a FoldShared, i32)> {
    match (&first.0.kind, &second.0.kind) {
        (
            PfKind::Fold {
                shared: a,
                index: i,
            },
            PfKind::Fold {
                shared: b,
                index: j,
            },
        ) if i == j && a.family == b.family => Some((&**a, &**b, *i)),
        _ => None,
    }
}

/// `PointFreeRule.CataFuseSame`: two folds that change one member of the family each fuse into one fold that composes
/// the two algebras at that member.
fn cata_fuse_same(first: &PointFree, second: &PointFree) -> Option<PointFree> {
    let (a, b, index) = same_family_folds(first, second)?;
    let mut algebra = Vec::with_capacity(a.algebra.len());
    let mut found_one = false;
    for (first_alg, second_alg) in a.algebra.iter().zip(b.algebra.iter()) {
        let (first_id, second_id) = (first_alg.view.is_nop(), second_alg.view.is_nop());
        if first_id && second_id {
            algebra.push(first_alg.clone());
        } else {
            if found_one || first_id || second_id {
                return None;
            }
            algebra.push(first_alg.compose(second_alg));
            found_one = true;
        }
    }
    Some(fold(a, algebra, index))
}

/// `PointFreeRule.CataFuseDifferent`: two folds that change disjoint members of the family, neither reaching the
/// members the other changes, fuse into one fold that takes each member's algebra from the fold that changes it.
fn cata_fuse_different(first: &PointFree, second: &PointFree) -> Option<PointFree> {
    let (a, b, index) = same_family_folds(first, second)?;
    let (mut first_modifies, mut second_modifies) = (BitSet::new(), BitSet::new());
    for (i, (first_alg, second_alg)) in a.algebra.iter().zip(b.algebra.iter()).enumerate() {
        let (first_id, second_id) = (first_alg.view.is_nop(), second_alg.view.is_nop());
        if !first_id && !second_id {
            return None;
        }
        if !first_id {
            first_modifies.set(i as i32);
        }
        if !second_id {
            second_modifies.set(i as i32);
        }
    }
    let mut algebra = Vec::with_capacity(a.algebra.len());
    for (first_alg, second_alg) in a.algebra.iter().zip(b.algebra.iter()) {
        if first_alg.rec_data.intersects(second_modifies)
            || second_alg.rec_data.intersects(first_modifies)
        {
            return None;
        }
        algebra.push(if first_alg.view.is_nop() {
            second_alg.clone()
        } else {
            first_alg.clone()
        });
    }
    Some(fold(a, algebra, index))
}

/// Two optic applications `Apply(ProfunctorTransformer(o), f)`.
fn app_pair<'a>(
    first: &'a PointFree,
    second: &'a PointFree,
) -> Option<(&'a TypedOptic, &'a PointFree, &'a TypedOptic, &'a PointFree)> {
    match (&first.0.kind, &second.0.kind) {
        (PfKind::App { optic: o1, f: f1 }, PfKind::App { optic: o2, f: f2 }) => {
            Some((o1, f1, o2, f2))
        }
        _ => None,
    }
}

/// `Functions.app(new ProfunctorTransformer(optic), f)`, or `f` itself for an empty optic.
fn cap_app(elements: &[crate::optic::Element], f: &PointFree) -> PointFree {
    if elements.is_empty() {
        f.clone()
    } else {
        PointFree::app(TypedOptic::from_elements(elements), f)
    }
}

/// `PointFreeRule.LensComp`: two optic applications that share their outermost optics apply the shared prefix once
/// around the composition of what is left.
fn lens_comp(first: &PointFree, second: &PointFree) -> Option<PointFree> {
    let (o1, f1, o2, f2) = app_pair(first, second)?;
    let (decomposed_first, decomposed_second) = (o1.elements(), o2.elements());
    let prefix = decomposed_first
        .iter()
        .zip(decomposed_second)
        .take_while(|(a, b)| a.optic == b.optic)
        .count();
    if prefix == 0 {
        return None;
    }
    if prefix == decomposed_first.len() && prefix == decomposed_second.len() {
        return Some(cap_app(decomposed_first, &PointFree::comp(f1, f2)));
    }
    let first_fork = cap_app(&decomposed_first[prefix..], f1);
    let second_fork = cap_app(&decomposed_second[prefix..], f2);
    Some(cap_app(
        &decomposed_first[..prefix],
        &PointFree::comp(&first_fork, &second_fork),
    ))
}

/// `PointFreeRule.SortProj`: a `proj2` application after a `proj1` one runs before it. The intermediate types are
/// `cap`'s, as in Java.
fn sort_proj(first: &PointFree, second: &PointFree) -> Option<PointFree> {
    let (o1, f1, o2, f2) = app_pair(first, second)?;
    if !matches!(o1.elements()[0].optic, Optic::Proj2)
        || !matches!(o2.elements()[0].optic, Optic::Proj1)
    {
        return None;
    }
    let input = o2.s_type();
    let output = o1.t_type();
    let middle = Type::product(
        output.as_product().expect("ProductType").0,
        input.as_product().expect("ProductType").1,
    );
    Some(PointFree::comp_raw(vec![
        PointFree::app(o2.cast_outer(middle.clone(), output.clone()), f2),
        PointFree::app(o1.cast_outer(input.clone(), middle), f1),
    ]))
}

/// `PointFreeRule.SortInj`: an `inj2` application after an `inj1` one runs before it.
fn sort_inj(first: &PointFree, second: &PointFree) -> Option<PointFree> {
    let (o1, f1, o2, f2) = app_pair(first, second)?;
    if !matches!(o1.elements()[0].optic, Optic::Inj2)
        || !matches!(o2.elements()[0].optic, Optic::Inj1)
    {
        return None;
    }
    let input = o2.s_type();
    let output = o1.t_type();
    let middle = Type::sum(
        output.as_sum().expect("SumType").0,
        input.as_sum().expect("SumType").1,
    );
    Some(PointFree::comp_raw(vec![
        PointFree::app(o2.cast_outer(middle.clone(), output.clone()), f2),
        PointFree::app(o1.cast_outer(input.clone(), middle), f1),
    ]))
}

/// `PointFreeRule.AppNest`: `app(o1, app(o2, f))` is `app(o1 ∘ o2, f)`.
fn app_nest(expr: &PointFree) -> Option<PointFree> {
    let PfKind::App { optic: o1, f } = &expr.0.kind else {
        return None;
    };
    let PfKind::App { optic: o2, f: arg } = &f.0.kind else {
        return None;
    };
    Some(PointFree::app(o1.compose(o2), arg))
}
