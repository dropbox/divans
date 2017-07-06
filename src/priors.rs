#![allow(unused)]
#![macro_escape]
use core;
use super::probability::{CDF2, CDF16, CDFDebug};
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};

macro_rules! define_prior_struct {
    // Syntax: define_prior_struct(StructName, BillingType,
    //                             billing_type1, count1, billing_type2, count2, ...);
    ($name: ident, $billing_type: ty, $($args:tt),*) => {
        // TODO: this struct should probably own/manage its allocated memory,
        // since it is required to be of a particular size.
        struct $name<T: CDFDebug + Default, AllocT: Allocator<T>> {
            priors: AllocT::AllocatedMemory
        }
        impl<T: CDFDebug + Default, AllocT: Allocator<T>> $name<T, AllocT> {
            #[inline]
            fn get(&mut self, billing: $billing_type, index: usize) -> &mut T {
                let offset = define_prior_struct_helper_offset!(billing; $($args),*);
                assert!(index < $name::<T, AllocT>::num_prior(&billing));
                &mut self.priors.slice_mut()[(offset as usize) + index]
            }
            // TODO: technically this does not depend on the template paramters.
            #[inline]
            fn num_all_priors() -> usize {
                sum_product_cdr!($($args),*) as usize
            }
            #[inline]
            fn num_prior(billing: &$billing_type) -> usize {
                (define_prior_struct_helper_product!(billing; $($args),*)) as usize
            }
            #[inline]
            fn num_billing_types() -> usize {
                count_tt!($($args),*) as usize
            }
        }
        #[cfg(feature="debug_entropy")]
        impl<T: CDFDebug + Default, AllocT: Allocator<T>> Drop for $name<T, AllocT> {
            fn drop(&mut self) {
                // Check for proper initialization.
                if self.priors.slice().len() != $name::<T, AllocT>::num_all_priors() {
                    return;
                }
                println!("[Summary for {}]", stringify!($name));
                for i in 0..$name::<T, AllocT>::num_billing_types() {
                    let billing = define_prior_struct_helper_select!(i; $($args),*);
                    let count = $name::<T, AllocT>::num_prior(&billing);
                    let mut num_cdfs_printed = 0usize;
                    for i in 0..count {
                        if num_cdfs_printed == 16 {
                            println!("  {:?}[...] : omitted", billing);
                            break;
                        }
                        let cdf = self.get(billing.clone(), i);
                        if cdf.used() {
                            println!("  {:?}[{}] : {}", billing, i, cdf.entropy());
                            num_cdfs_printed += 1;
                        }
                    }
                }
            }
        }
    };
}

macro_rules! define_prior_struct_helper_offset {
    ($billing: expr; ($type: expr, $($args: expr),*)) => { 0 };  // should panic if billing != type
    ($billing: expr; ($type: expr, $($args: expr),*), $($more:tt),*) => {
        (($billing != $type) as u32) * (product!($($args),*) + define_prior_struct_helper_offset!($billing; $($more),*))
    };
}

macro_rules! define_prior_struct_helper_product {
    ($billing: expr; ($type: expr, $($args: expr),*)) => { product!($($args),*) };  // should panic if billing != type
    ($billing: expr; ($type: expr, $($args: expr),*), $($more:tt),*) => {
        if *$billing == $type { product!($($args),*) } else { define_prior_struct_helper_product!($billing; $($more),*) }
    };
}

macro_rules! define_prior_struct_helper_select {
    ($index: expr; ($type: expr, $($args: expr),*)) => { $type };  // should panic if billing != type
    ($index: expr; ($type: expr, $($args: expr),*), $($more:tt),*) => {
        if $index == 0 { $type } else { define_prior_struct_helper_select!(($index - 1); $($more),*) }
    };
}

// Given a list of tuples, compute the product of all but the first number for each tuple,
// and report the sum of the said products.
macro_rules! sum_product_cdr {
    (($a: expr, $($args: expr),*)) => { product!($($args),*) };
    (($a: expr, $($args: expr),*), $($more: tt),*) => { product!($($args),*) + sum_product_cdr!($($more),*) };
}

macro_rules! product {
    ($a: expr) => { ($a as u32) };
    ($a: expr, $b: expr) => { (($a * $b) as u32) };
    ($a: expr, $($args: expr),*) => { ($a as u32) * product!($($args),*) };
}

macro_rules! count_tt {
    ($args: tt) => { 1 };
    ($args: tt, $($more: tt),*) => { (1 + count_tt!($($more),*)) };
}

mod test {
    use super::{Allocator, CDF2, CDFDebug, SliceWrapper, SliceWrapperMut};
    use alloc::HeapAlloc;

    #[derive(PartialEq, Eq, Clone, Copy, Debug)]
    enum PriorType { Foo, Bar, Cat }
    define_prior_struct!(TestPriorSet, PriorType,
                         (PriorType::Foo, 5, 2), (PriorType::Bar, 6), (PriorType::Cat, 3));

    type TestPriorSetImpl = TestPriorSet<CDF2, HeapAlloc<CDF2>>;

    #[test]
    fn test_macro_product() {
        assert_eq!(product!(5), 5);
        assert_eq!(product!(2, 3, 4), 24);
    }

    #[test]
    fn test_macro_sum_product_cdr() {
        assert_eq!(sum_product_cdr!((1, 2)), 2);
        assert_eq!(sum_product_cdr!((1, 2, 3)), 6);
        assert_eq!(sum_product_cdr!((1, 2, 3), (2, 3, 4)), 18);
    }

    #[test]
    fn test_num_prior() {
        let cases : [(PriorType, usize); 3] = [(PriorType::Foo, 5 * 2),
                                               (PriorType::Bar, 6),
                                               (PriorType::Cat, 3)];
        let mut expected_sum = 0usize;
        for &(t, expected_count) in cases.iter() {
            assert_eq!(TestPriorSetImpl::num_prior(&t), expected_count);
            expected_sum += expected_count;
        }
        assert_eq!(TestPriorSetImpl::num_all_priors(), expected_sum);
    }

    #[test]
    fn test_get() {
        let mut allocator = HeapAlloc::<CDF2>::new(CDF2::default());
        let mut prior_set = TestPriorSetImpl {
            priors: allocator.alloc_cell(TestPriorSetImpl::num_all_priors()),
        };
        let prior_types : [PriorType; 3] = [PriorType::Foo, PriorType::Bar, PriorType::Cat];
        // Check that all priors are initialized to default.
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(&t) {
                let cdf = prior_set.get(t, i);
                assert_eq!(cdf.prob, 128u8);
            }
        }

        // Use the priors, updating them by varying degrees.
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(&t) {
                let mut cdf = prior_set.get(t, i);
                for j in 0..i {
                    cdf.blend(true);
                }
            }
        }

        // Ascertain that the priors were updated the proper # of times.
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(&t) {
                let cdf = prior_set.get(t, i);
                let mut baseline = CDF2::default();
                for j in 0..i {
                    baseline.blend(true);
                }
                assert_eq!(cdf.prob, baseline.prob);
            }
        }
    }
}
