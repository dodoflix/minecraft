//! `RecursiveTypeFamily`: the mutually recursive types of a schema (`registerType(recursive = true, ..)`), their
//! `Mu` points, and the cata-based `everywhere` rewrite over them.
//!
//! Java interns the family *template* (`TEMPLATE_INTERNER`) and compares families by template identity; families are
//! interned here by template equality for the same effect (and so that equal families share their point types and
//! unfoldings; the interner lives for the process, bounded by the number of distinct templates ever built).

use crate::optic::TypedOptic;
use crate::rewrite::{BitSet, FoldShared, OptimizationRule, RewriteResult, TypeRewriteRule, View};
use crate::template::Template;
use crate::ty::{FindError, Matcher, Type};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

pub struct RecursiveTypeFamily {
    name: String,
    template: Template,
    size: i32,
    types: Mutex<HashMap<i32, Type>>,
}

static FAMILIES: LazyLock<Mutex<HashMap<Template, Arc<RecursiveTypeFamily>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl PartialEq for RecursiveTypeFamily {
    fn eq(&self, o: &RecursiveTypeFamily) -> bool {
        std::ptr::eq(self, o) || self.template == o.template
    }
}
impl Eq for RecursiveTypeFamily {}

impl RecursiveTypeFamily {
    /// `new RecursiveTypeFamily(name, template)` (interned by template).
    pub fn new(name: &str, template: Template) -> Arc<RecursiveTypeFamily> {
        let mut families = FAMILIES.lock().unwrap();
        if let Some(f) = families.get(&template) {
            return f.clone();
        }
        let size = template.size();
        let family = Arc::new(RecursiveTypeFamily {
            name: name.to_string(),
            template: template.clone(),
            size,
            types: Mutex::new(HashMap::new()),
        });
        families.insert(template, family.clone());
        family
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn template(&self) -> &Template {
        &self.template
    }
    pub fn size(&self) -> i32 {
        self.size
    }

    /// `family.apply(index)`: the `RecursivePointType` of that index.
    pub fn apply(self: &Arc<Self>, index: i32) -> Type {
        assert!(index >= 0, "IndexOutOfBounds: {index}");
        if let Some(t) = self.types.lock().unwrap().get(&index) {
            return t.clone();
        }
        let t = Type::recursive_point(self.clone(), index);
        self.types.lock().unwrap().entry(index).or_insert(t).clone()
    }

    /// `family.buildMuType(newType, newFamily)`.
    pub(crate) fn build_mu_type(
        self: &Arc<Self>,
        new_type: &Type,
        new_family: Option<&Arc<RecursiveTypeFamily>>,
    ) -> Type {
        let new_family = match new_family {
            Some(f) => f.clone(),
            None => {
                let new_template = new_type.template();
                if self.template == new_template {
                    self.clone()
                } else {
                    RecursiveTypeFamily::new(&format!("ruled {}", self.name), new_template)
                }
            }
        };
        for i in 0..new_family.size {
            let t = new_family.apply(i);
            if new_type.equals_with(&t.unfold(), true, false) {
                return t;
            }
        }
        panic!("Couldn't determine the new type properly");
    }

    /// `family.findType(index, aType, bType, matcher, recurse)`.
    pub(crate) fn find_type(
        self: &Arc<Self>,
        index: i32,
        matcher: &Matcher,
        recurse: bool,
    ) -> Result<TypedOptic, FindError> {
        let s_type = self.apply(index);
        let optic = s_type.unfold().find_type(matcher, false)?;
        let new_family = RecursiveTypeFamily::new(&self.name, optic.t_type().template());
        let t_type = new_family.apply(index);
        if recurse {
            panic!(
                "findType(recurse = true) through a recursive family is not ported (no vanilla fix uses \
                 Typed.updateRecursive / getAll with recursion)"
            );
        }
        Ok(optic.cast_outer(s_type, t_type))
    }

    /// `family.everywhere(index, rule, optimizationRule)`.
    pub(crate) fn everywhere(
        self: &Arc<Self>,
        index: i32,
        rule: &TypeRewriteRule,
        opt: OptimizationRule,
    ) -> Option<RewriteResult> {
        let source_type = self.apply(index).unfold();
        let source_view = source_type
            .everywhere(rule, opt, false, false)
            .unwrap_or_else(|| RewriteResult::nop(&source_type));
        let new_type = self.build_mu_type(source_view.view.new_type(), None);
        let new_family = new_type
            .family()
            .expect("buildMuType returns a recursion point")
            .clone();
        let mut views: Vec<RewriteResult> = Vec::with_capacity(self.size as usize);
        let mut found_any = false;
        for i in 0..self.size {
            let ty = self.apply(i);
            let unfold = ty.unfold();
            let view = unfold
                .everywhere(rule, opt, false, true)
                .unwrap_or_else(|| RewriteResult::nop(&unfold));
            let nop1 = view.view.is_nop();
            let new_mu_type = self.build_mu_type(view.view.new_type(), Some(&new_family));
            let nop = self.cap2(&mut views, rule, nop1, view, &new_mu_type);
            found_any = found_any || !nop;
        }
        if !found_any {
            return None;
        }
        let fold = FoldShared::fold_fn(self, Arc::new(views), &new_family)(index);
        Some(RewriteResult::create(View(fold.view.0), fold.rec_data))
    }

    /// `family.cap2(..)`: returns whether the member stayed a nop.
    fn cap2(
        self: &Arc<Self>,
        views: &mut Vec<RewriteResult>,
        rule: &TypeRewriteRule,
        nop: bool,
        view: RewriteResult,
        new_type: &Type,
    ) -> bool {
        let mut nop = nop;
        let mut view = view;
        let unfolded = new_type.unfold();
        let new_view =
            RewriteResult::create(View::in_(new_type, &unfolded), BitSet::new()).compose(&view);
        if let Some(rewrite) = rule.rewrite(new_view.view.new_type())
            && !rewrite.view.is_nop()
        {
            nop = false;
            view = rewrite.compose(&new_view);
        }
        views.push(view);
        nop
    }
}
