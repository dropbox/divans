#![allow(unused)]
#![macro_escape]
use core;
use super::probability::{CDF2, CDF16};
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};

macro_rules! define_prior_struct {
    // Syntax: define_prior_struct(StructName, BillingType,
    //                             billing_type1, count1, billing_type2, count2, ...);
    ($name: ident, $billing_type: ty, $($args:tt),*) => {
        // TODO: this struct should probably own/manage its allocated memory,
        // since it is required to be of a particular size.
        struct $name<T, AllocT: Allocator<T>> {
            priors: AllocT::AllocatedMemory
        }
        impl<T, AllocT: Allocator<T>> $name<T, AllocT> {
            #[inline]
            fn get(&mut self, billing: $billing_type, index: usize) -> &mut T {
                let offset = define_prior_struct_helper_offset!(billing; $($args),*);
                &mut self.priors.slice_mut()[(offset as usize) + index]
            }
            // TODO: technically this does not depend on the template paramters.
            fn num_priors() -> usize {
                sum_cdr!($($args),*)
            }
        }
    };
}

macro_rules! define_prior_struct_helper_offset {
    ($billing: expr; ($type: expr, $count: expr)) => { 0 };  // should panic if billing != type
    ($billing: expr; ($type: expr, $count: expr), $($more:tt),*) => {
        (($billing != $type) as u32) * ($count + define_prior_struct_helper_offset!($billing; $($more),*))
    };
}

macro_rules! sum_cdr {
    (($a: expr, $b: expr)) => { ($b as usize) };
    (($a: expr, $b: expr), $($args:tt),*) => { $b + sum_cdr!($($args),*) };
}

mod test {
    use super::{Allocator, CDF2, SliceWrapperMut};
    use alloc::HeapAlloc;

    #[derive(PartialEq, Eq, Clone, Copy)]
    enum PriorType { Foo, Bar, Cat }
    define_prior_struct!(TestPriorSet, PriorType,
                         (PriorType::Foo, 5), (PriorType::Bar, 6), (PriorType::Cat, 3));

    #[test]
    fn test_num_priors() {
        assert_eq!(TestPriorSet::<CDF2, HeapAlloc<CDF2>>::num_priors(), 5 + 6 + 3);
    }

    #[test]
    fn test_get() {
        let mut allocator = HeapAlloc::<CDF2>::new(CDF2::default());
        let mut prior_set = TestPriorSet::<CDF2, HeapAlloc<CDF2>> {
            priors: allocator.alloc_cell(TestPriorSet::<CDF2, HeapAlloc<CDF2>>::num_priors()),
        };
        let cases : [(PriorType, usize);3] = [(PriorType::Foo, 5),
                                              (PriorType::Bar, 6),
                                              (PriorType::Cat, 3)];
        // Check that all priors are initialized to default.
        for case in cases.iter() {
            for i in 0..(case.1) {
                let mut cdf = prior_set.get(case.0, i);
                assert_eq!(cdf.prob, 128u8);
            }
        }

        // Use the priors, updating them by varying degrees.
        for case in cases.iter() {
            for i in 0..(case.1) {
                let mut cdf = prior_set.get(case.0, i);
                for j in 0..i {
                    cdf.blend(true);
                }
            }
        }

        // Ascertain that the priors were updated the proper # of times.
        for case in cases.iter() {
            for i in 0..(case.1) {
                let mut cdf = prior_set.get(case.0, i);
                let mut baseline = CDF2::default();
                for j in 0..i {
                    baseline.blend(true);
                }
                assert_eq!(cdf.prob, baseline.prob);
            }
        }
    }
}
