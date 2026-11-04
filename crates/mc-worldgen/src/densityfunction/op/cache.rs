//! `CacheFunction`: marker replaced by `PreparedCache` during compilation (never compiled directly).
use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::sampler::SharedSampler;
use crate::util::Interval;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CacheFunction {
    pub input: DensityFunction,
}

impl DfOp for CacheFunction {
    fn compile_sampler(&self, _ctx: &dyn CompileContext) -> SharedSampler {
        panic!("Cannot compile cache before it has been deduplicated")
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let input = rule.rewrite(&self.input);
        if input.ptr_eq(&self.input) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Cache(CacheFunction { input }))
        }
    }
    fn range(&self) -> Interval {
        self.input.range()
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_children_preserves_identity_when_unchanged() {
        let cache = DensityFunction::constant(1.0).cache();
        assert!(
            cache
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&cache)
        );
        let rewritten =
            cache.rewrite_children(&|_: &DensityFunction| DensityFunction::constant(2.0));
        assert!(!rewritten.ptr_eq(&cache));
        assert_eq!(rewritten, DensityFunction::constant(2.0).cache());
    }

    #[test]
    #[should_panic(expected = "Cannot compile cache before it has been deduplicated")]
    fn compile_sampler_panics() {
        DensityFunction::constant(1.0)
            .cache()
            .compile_sampler(&crate::densityfunction::op::slice::test_support::NoContext);
    }
}
