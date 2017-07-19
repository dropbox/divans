#![allow(unused)]
#![macro_escape]
use core;
use super::probability::{BaseCDF, CDF2, CDF16, SliceRefCDF16};
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};

pub trait PriorMultiIndex {
    fn expand(&self) -> (usize, usize, usize, usize);
    fn expand_into_parent(&self) -> (usize, usize, usize, usize);
    fn num_dimensions() -> usize;
}

impl PriorMultiIndex for (usize,) {
    fn expand(&self) -> (usize, usize, usize, usize) { (self.0 + 1, 0, 0, 0) }
    fn expand_into_parent(&self) -> (usize, usize, usize, usize) { (0, 0, 0, 0) }
    fn num_dimensions() -> usize { 1usize }
}

impl PriorMultiIndex for (usize, usize) {
    fn expand(&self) -> (usize, usize, usize, usize) { (self.0 + 1, self.1 + 1, 0, 0) }
    fn expand_into_parent(&self) -> (usize, usize, usize, usize) { (self.0 + 1, 0, 0, 0) }
    fn num_dimensions() -> usize { 2usize }
}

impl PriorMultiIndex for (usize, usize, usize) {
    fn expand(&self) -> (usize, usize, usize, usize) { (self.0 + 1, self.1 + 1, self.2 + 1, 0) }
    fn expand_into_parent(&self) -> (usize, usize, usize, usize) { (self.0 + 1, self.1 + 1, 0, 0) }
    fn num_dimensions() -> usize { 3usize }
}

impl PriorMultiIndex for (usize, usize, usize, usize) {
    fn expand(&self) -> (usize, usize, usize, usize) { (self.0 + 1, self.1 + 1, self.2 + 1, self.3 + 1) }
    fn expand_into_parent(&self) -> (usize, usize, usize, usize) { (self.0 + 1, self.1 + 1, self.2 + 1, 0) }
    fn num_dimensions() -> usize { 4usize }
}

pub trait PriorCollection<T: CDF16, AllocT: Allocator<T>, B> {
    fn get<I: PriorMultiIndex>(&mut self, billing: B, index: I) -> &mut T;
    fn get_chained<'a, I: PriorMultiIndex>(&'a mut self, billing: B, index: I) -> SliceRefCDF16<'a, T>;
    fn get_with_raw_index(&mut self, billing: B, index: usize) -> &mut T;
    fn num_all_priors() -> usize;
    fn num_prior(billing: &B) -> usize;
    fn num_dimensions(billing: &B) -> usize;
    fn num_billing_types() -> usize;
    fn index_to_billing_type(index: usize) -> B;
    fn summarize(&mut self) {}
}

macro_rules! define_prior_struct {
    // Syntax: define_prior_struct(StructName, BillingType,
    //                             billing_type1, count1, billing_type2, count2, ...);
    ($name: ident, $billing_type: ty, $($args:tt),*) => {
        // TODO: this struct should probably own/manage its allocated memory,
        // since it is required to be of a particular size.
        struct $name<T: CDF16, AllocT: Allocator<T>> {
            priors: AllocT::AllocatedMemory
        }
        impl<T: CDF16, AllocT: Allocator<T>> $name<T, AllocT> {
            #[inline]
            fn get_prior_slice(&mut self, billing: $billing_type) -> &mut [T] {
                let offset_type = define_prior_struct_helper_offset!(billing; $($args),*) as usize;
                &mut self.priors.slice_mut()[offset_type..(offset_type + Self::num_prior(&billing))]
            }
            #[inline]
            fn flatten_expanded_index(billing: &$billing_type, expanded_index: (usize, usize, usize, usize)) -> usize {
                let expanded_dim : (usize, usize, usize, usize) = (define_prior_struct_helper_select_dim!(billing; 0; $($args),*),
                                                                   define_prior_struct_helper_select_dim!(billing; 1; $($args),*),
                                                                   define_prior_struct_helper_select_dim!(billing; 2; $($args),*),
                                                                   define_prior_struct_helper_select_dim!(billing; 3; $($args),*));
                debug_assert!(expanded_index.0 <= expanded_dim.0 &&
                              expanded_index.1 <= expanded_dim.1 &&
                              expanded_index.2 <= expanded_dim.2 &&
                              expanded_index.3 <= expanded_dim.3,
                              "Index out of bounds for {:?}", billing);
                let offset_index = expanded_index.0 +
                    (expanded_dim.0 + 1) * (expanded_index.1 +
                                            (expanded_dim.1 + 1) * (expanded_index.2 + (expanded_dim.2 + 1) * expanded_index.3));
                debug_assert!(offset_index < Self::num_prior(&billing),
                              "Offset from the index is out of bounds for {:?}", billing);
                return offset_index;
            }
        }
        impl<T: CDF16, AllocT: Allocator<T>> PriorCollection<T, AllocT, $billing_type> for $name<T, AllocT> {
            #[inline]
            fn get_with_raw_index(&mut self, billing: $billing_type, index: usize) -> &mut T {
                &mut self.get_prior_slice(billing)[index]
            }
            #[inline]
            fn get<I: PriorMultiIndex>(&mut self, billing: $billing_type, index: I) -> &mut T {
                // Check the dimensionality.
                let expected_dim = Self::num_dimensions(&billing);
                debug_assert_eq!(I::num_dimensions(), expected_dim,
                                 "Index has {} dimensions but {} is expected for {:?}",
                                 I::num_dimensions(), expected_dim, billing);
                // Compute the offset arising from the index.
                let offset_index = Self::flatten_expanded_index(&billing, index.expand());
                &mut self.get_prior_slice(billing)[offset_index]
            }
            #[inline]
            fn get_chained<'a, I: PriorMultiIndex>(&'a mut self, billing: $billing_type, index: I) -> SliceRefCDF16<'a, T> {
                // Check the dimensionality.
                let expected_dim = Self::num_dimensions(&billing);
                debug_assert_eq!(I::num_dimensions(), expected_dim,
                                 "Index has {} dimensions but {} is expected for {:?}",
                                 I::num_dimensions(), expected_dim, billing);
                let offset_index = Self::flatten_expanded_index(&billing, index.expand());
                let parent_offset_index = Self::flatten_expanded_index(&billing, index.expand_into_parent());
                let priors = self.get_prior_slice(billing);
                let active: bool = priors[offset_index].num_samples().unwrap_or(0) > 4;
                SliceRefCDF16::<'a, T>::new(priors,
                                            if active { offset_index } else { parent_offset_index },
                                            if active { parent_offset_index } else { offset_index })
            }
            // TODO: technically this does not depend on the template paramters.
            #[inline]
            fn num_all_priors() -> usize {
                sum_product_cdr_post_increment!($($args),*) as usize
            }
            #[inline]
            fn num_prior(billing: &$billing_type) -> usize {
                (define_prior_struct_helper_product_post_increment!(billing; $($args),*)) as usize
            }
            #[inline]
            fn num_dimensions(billing: &$billing_type) -> usize {
                (define_prior_struct_helper_dimensionality!(billing; $($args),*)) as usize
            }

            fn num_billing_types() -> usize {
                count_expr!($($args),*) as usize
            }
            fn index_to_billing_type(index: usize) -> $billing_type {
                define_prior_struct_helper_select_type!(index; $($args),*)
            }

            #[cfg(feature="billing")]
            #[cfg(feature="debug_entropy")]
            fn summarize(&mut self) {
                // Check for proper initialization.
                if self.priors.slice().len() != Self::num_all_priors() {
                    return;
                }
                println!("[Summary for {}]", stringify!($name));
                for i in 0..Self::num_billing_types() {
                    let billing = Self::index_to_billing_type(i as usize);
                    let count = Self::num_prior(&billing);
                    let mut num_cdfs_printed = 0usize;
                    for i in 0..count {
                        if num_cdfs_printed == 16 {
                            println!("  {:?}[...] : omitted", billing);
                            break;
                        }
                        let cdf = self.get_with_raw_index(billing.clone(), i);
                        let true_entropy = cdf.true_entropy();
                        let rolling_entropy = cdf.rolling_entropy();
                        let num_samples = cdf.num_samples();
                        let encoding_cost = cdf.encoding_cost();
                        if cdf.used() && true_entropy.is_some() && rolling_entropy.is_some() &&
                            num_samples.is_some() && encoding_cost.is_some() {
                                println!("  {:?}[{}] : {:1.5} (True entropy: {:1.5}, Rolling entropy: {:1.5}, Final entropy: {:1.5}, #: {})",
                                         billing, i,
                                         encoding_cost.unwrap() / (num_samples.unwrap() as f64),
                                         true_entropy.unwrap(), rolling_entropy.unwrap(), cdf.entropy(), num_samples.unwrap());
                                num_cdfs_printed += 1;
                            }
                    }
                }
            }
        }
        #[cfg(feature="billing")]
        impl<T: CDF16, AllocT: Allocator<T>> Drop for $name<T, AllocT> {
            fn drop(&mut self) {
                self.summarize();
            }
        }
    };
}

macro_rules! define_prior_struct_helper_offset {
    ($billing: expr; ($typ: expr, $($args: expr),*)) => { 0 };  // should panic if billing != type
    ($billing: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        (($billing != $typ) as u32) * (product_post_increment!($($args),*) +
                                       define_prior_struct_helper_offset!($billing; $($more),*))
    };
}

macro_rules! define_prior_struct_helper_product_post_increment {
    ($billing: expr; ($typ: expr, $($args: expr),*)) => { product_post_increment!($($args),*) };  // should panic if billing != type
    ($billing: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        if *$billing == $typ { product_post_increment!($($args),*) } else {
            define_prior_struct_helper_product_post_increment!($billing; $($more),*)
        }
    };
}

macro_rules! define_prior_struct_helper_dimensionality {
    ($billing: expr; ($typ: expr, $($args: expr),*)) => { count_expr!($($args),*) };  // should panic if billing != type
    ($billing: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        if *$billing == $typ { count_expr!($($args),*) } else {
            define_prior_struct_helper_dimensionality!($billing; $($more),*)
        }
    };
}

macro_rules! define_prior_struct_helper_select_type {
    ($index: expr; ($typ: expr, $($args: expr),*)) => { $typ };  // should panic if billing != type
    ($index: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        if $index == 0 { $typ } else {
            define_prior_struct_helper_select_type!(($index - 1); $($more),*)
        }
    };
}

macro_rules! define_prior_struct_helper_select_dim {
    ($billing: expr; $index: expr; ($typ: expr, $($args: expr),*)) => { select_expr!($index; 1; $($args),*) };  // should panic if billing != type
    ($billing: expr; $index: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        if *$billing == $typ { select_expr!($index; 1; $($args),*) } else { define_prior_struct_helper_select_dim!($billing; $index; $($more),*) }
    };
}

// Given a list of tuples, compute the product of all but the first number for each tuple,
// and report the sum of the said products.
macro_rules! sum_product_cdr_post_increment {
    (($a: expr, $($args: expr),*)) => { product_post_increment!($($args),*) };
    (($a: expr, $($args: expr),*), $($more: tt),*) => {
        product_post_increment!($($args),*) + sum_product_cdr_post_increment!($($more),*) };
}

macro_rules! product_post_increment {
    ($a: expr) => { ($a as u32 + 1) };
    ($a: expr, $b: expr) => { ((($a + 1) * ($b + 1)) as u32) };
    ($a: expr, $($args: expr),*) => { ($a as u32 + 1) * product_post_increment!($($args),*) };
}

macro_rules! count_expr {
    ($args: expr) => { 1 };
    ($args: expr, $($more: expr),*) => { (1 + count_expr!($($more),*)) };
}

macro_rules! select_expr {
    ($index: expr; $fallback: expr; ) => { $fallback };
    ($index: expr; $fallback: expr; $val: expr) => { if $index == 0 { $val } else { $fallback } };
    ($index: expr; $fallback: expr; $val: expr, $($more: expr),*) => {
        if $index == 0 { $val } else { select_expr!($index - 1; $fallback; $($more),*) }
    }
}

mod test {
    use core;
    use super::{
        Allocator,
        BaseCDF,
        CDF16,
        PriorCollection,
        PriorMultiIndex,
        SliceRefCDF16,
        SliceWrapper,
        SliceWrapperMut
    };
    use probability::{FrequentistCDF16, Speed};
    use alloc::HeapAlloc;

    #[derive(PartialEq, Eq, Clone, Copy, Debug)]
    enum PriorType { Foo, Bar, Cat }
    define_prior_struct!(TestPriorSet, PriorType,
                         (PriorType::Foo, 5, 8, 2), (PriorType::Bar, 6, 2), (PriorType::Cat, 3));

    type TestPriorSetImpl = TestPriorSet<FrequentistCDF16, HeapAlloc<FrequentistCDF16>>;

    #[test]
    fn test_macro_product_post_increment() {
        assert_eq!(product_post_increment!(5), 6);
        assert_eq!(product_post_increment!(2, 3, 4), 3 * 4 * 5);
    }

    #[test]
    fn test_macro_sum_product_cdr_post_increment() {
        assert_eq!(sum_product_cdr_post_increment!(("a", 2)), 3);
        assert_eq!(sum_product_cdr_post_increment!(("a", 2, 3)), 3 * 4);
        assert_eq!(sum_product_cdr_post_increment!(("a", 2, 3), ("b", 3, 4)),
                   (3 * 4) + (4 * 5));
    }

    #[test]
    fn test_macro_select_expr() {
        assert_eq!(select_expr!(0; 1; 7, 8, 9), 7);
        assert_eq!(select_expr!(1; 1; 7, 8, 9), 8);
        assert_eq!(select_expr!(2; 1; 7, 8, 9), 9);
        assert_eq!(select_expr!(3; 1; 7, 8, 9), 1);
    }

    #[test]
    fn test_macro_count_expr() {
        assert_eq!(count_expr!(3), 1);
        assert_eq!(count_expr!(3, 4), 2);
        assert_eq!(count_expr!(3, 4, 5), 3);
    }

    #[test]
    fn test_num_prior() {
        let cases : [(PriorType, usize); 3] = [(PriorType::Foo, (5+1) * (8+1) * (2+1)),
                                               (PriorType::Bar, (6+1) * (2+1)),
                                               (PriorType::Cat, (3+1))];
        let mut expected_sum = 0usize;
        for &(t, expected_count) in cases.iter() {
            assert_eq!(TestPriorSetImpl::num_prior(&t), expected_count);
            expected_sum += expected_count;
        }
        assert_eq!(TestPriorSetImpl::num_all_priors(), expected_sum);
    }

    #[test]
    fn test_num_dimensions() {
        let cases : [(PriorType, usize); 3] = [(PriorType::Foo, 3),
                                               (PriorType::Bar, 2),
                                               (PriorType::Cat, 1)];
        for &(t, expected_dims) in cases.iter() {
            assert_eq!(TestPriorSetImpl::num_dimensions(&t), expected_dims);
        }
    }

    #[test]
    fn test_billing_types() {
        assert_eq!(TestPriorSetImpl::num_billing_types(), 3);
        assert_eq!(TestPriorSetImpl::index_to_billing_type(0), PriorType::Foo);
        assert_eq!(TestPriorSetImpl::index_to_billing_type(1), PriorType::Bar);
        assert_eq!(TestPriorSetImpl::index_to_billing_type(2), PriorType::Cat);
    }

    #[test]
    fn test_get_with_raw_index() {
        let mut allocator = HeapAlloc::<FrequentistCDF16>::new(FrequentistCDF16::default());
        let mut prior_set = TestPriorSetImpl {
            priors: allocator.alloc_cell(TestPriorSetImpl::num_all_priors()),
        };
        let prior_types : [PriorType; 3] = [PriorType::Foo, PriorType::Bar, PriorType::Cat];
        // Check that all priors are initialized to default.
        let reference = FrequentistCDF16::default();
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(&t) {
                let cdf = prior_set.get_with_raw_index(t, i);
                for j in 0..16 {
                    assert_eq!(cdf.cdf(j), reference.cdf(j));
                }
            }
        }

        // Use the priors, updating them by varying degrees.
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(&t) {
                let mut cdf = prior_set.get_with_raw_index(t, i);
                for j in 0..i {
                    cdf.blend((j as u8) % 16, Speed::MED);
                }
            }
        }

        // Ascertain that the priors were updated the proper # of times.
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(&t) {
                let cdf = prior_set.get_with_raw_index(t, i);
                let mut baseline = FrequentistCDF16::default();
                for j in 0..i {
                    baseline.blend((j as u8) % 16, Speed::MED);
                }
                for j in 0..16 {
                    assert_eq!(cdf.cdf(j), baseline.cdf(j));
                }
            }
        }
    }

    #[test]
    fn test_get_tuple() {
        let mut allocator = HeapAlloc::<FrequentistCDF16>::new(FrequentistCDF16::default());
        let mut prior_set = TestPriorSetImpl {
            priors: allocator.alloc_cell(TestPriorSetImpl::num_all_priors()),
        };
        for i in 0..5 {
            for j in 0..8 {
                for k in 0..2 {
                    let mut cdf = prior_set.get(PriorType::Foo, (i, j, k));
                    let mut baseline = FrequentistCDF16::default();
                    for l in 0..i {
                        cdf.blend(l as u8, Speed::MED);
                        baseline.blend(l as u8, Speed::MED);
                        for symbol in 0..16 {
                            assert_eq!(cdf.cdf(symbol), baseline.cdf(symbol));
                        }
                    }
                    for l in 0..j {
                        cdf.blend((l ^ 0xf) as u8, Speed::MED);
                        baseline.blend((l ^ 0xf) as u8, Speed::MED);
                        for symbol in 0..16 {
                            assert_eq!(cdf.cdf(symbol), baseline.cdf(symbol));
                        }
                    }
                    for l in 0..k {
                        cdf.blend(l as u8, Speed::MED);
                        baseline.blend(l as u8, Speed::MED);
                        for symbol in 0..16 {
                            assert_eq!(cdf.cdf(symbol), baseline.cdf(symbol));
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[should_panic]
    fn test_get_bad_tuple_index() {
        let mut allocator = HeapAlloc::<FrequentistCDF16>::new(FrequentistCDF16::default());
        let mut prior_set = TestPriorSetImpl {
            priors: allocator.alloc_cell(TestPriorSetImpl::num_all_priors()),
        };
        prior_set.get(PriorType::Bar, (6, 1));
    }

    #[test]
    #[should_panic]
    fn test_get_bad_tuple_dimensionality() {
        let mut allocator = HeapAlloc::<FrequentistCDF16>::new(FrequentistCDF16::default());
        let mut prior_set = TestPriorSetImpl {
            priors: allocator.alloc_cell(TestPriorSetImpl::num_all_priors()),
        };
        prior_set.get(PriorType::Bar, (0, 0, 0));
    }
}
