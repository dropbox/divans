#![allow(unused)]
#![macro_escape]
use core;
use super::probability::{CDF2, CDF16, Entropy};
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};

macro_rules! define_prior_struct {
    // Syntax: define_prior_struct(StructName, BillingType,
    //                             billing_type1, count1, billing_type2, count2, ...);
    ($name: ident, $billing_type: ty, $($args:tt),*) => {
        // TODO: this struct should probably own/manage its allocated memory,
        // since it is required to be of a particular size.
        struct $name<T: Entropy, AllocT: Allocator<T>> {
            priors: AllocT::AllocatedMemory
        }
        impl<T: Entropy, AllocT: Allocator<T>> $name<T, AllocT> {
            #[inline]
            fn get(&mut self, billing: $billing_type, index: usize) -> &mut T {
                let offset = define_prior_struct_helper_offset!(billing; $($args),*);
                &mut self.priors.slice_mut()[(offset as usize) + index]
            }
            // TODO: technically this does not depend on the template paramters.
            #[inline]
            fn num_all_priors() -> usize {
                sum_cdr!($($args),*)
            }
            #[inline]
            fn num_prior(billing: $billing_type) -> usize {
                (define_prior_struct_helper_select!(billing; $($args),*)) as usize
            }
        }
        #[cfg(feature="debug_entropy")]
        impl<T: Entropy, AllocT: Allocator<T>> Drop for $name<T, AllocT> {
            fn drop(&mut self) {
                // Check for proper initialization.
                if self.priors.slice().len() != $name::<T, AllocT>::num_all_priors() {
                    return;
                }
                println!("[Summary for {}]", stringify!($name));
                for arg in [$($args),*].into_iter() {
                    let ref billing = arg.0;
                    let count = arg.1;
                    for i in 0..count {
                        if i == 16 {
                            println!("  {:?}[..] : omitted", billing);
                            break;
                        }
                        let ent = self.get(billing.clone(), i).entropy();
                        println!("  {:?}[{:2}] : {}", billing, i, ent);
                    }
                }
            }
        }
    };
}

macro_rules! define_prior_struct_helper_offset {
    ($billing: expr; ($ty: expr, $count: expr)) => { 0 };  // should panic if billing != type
    ($billing: expr; ($ty: expr, $count: expr), $($more:tt),*) => {
        (($billing != $ty) as u32) * ($count + define_prior_struct_helper_offset!($billing; $($more),*))
    };
}

macro_rules! define_prior_struct_helper_select {
    ($billing: expr; ($ty: expr, $count: expr)) => { $count };  // should panic if billing != type
    ($billing: expr; ($ty: expr, $count: expr), $($more:tt),*) => {
        if $billing == $ty { $count } else { define_prior_struct_helper_select!($billing; $($more),*) }
    };
}

macro_rules! sum_cdr {
    (($a: expr, $b: expr)) => { ($b as usize) };
    (($a: expr, $b: expr), $($args:tt),*) => { $b + sum_cdr!($($args),*) };
}

mod test {
    use super::{Allocator, CDF2, Entropy, SliceWrapper, SliceWrapperMut};
    use alloc::HeapAlloc;

    #[derive(PartialEq, Eq, Clone, Copy, Debug)]
    enum PriorType { Foo, Bar, Cat }
    define_prior_struct!(TestPriorSet, PriorType,
                         (PriorType::Foo, 5), (PriorType::Bar, 6), (PriorType::Cat, 3));

    type TestPriorSetImpl = TestPriorSet<CDF2, HeapAlloc<CDF2>>;

    #[test]
    fn test_num_prior() {
        let cases : [(PriorType, usize); 3] = [(PriorType::Foo, 5),
                                               (PriorType::Bar, 6),
                                               (PriorType::Cat, 3)];
        let mut expected_sum = 0usize;
        for &(t, expected_count) in cases.iter() {
            assert_eq!(TestPriorSetImpl::num_prior(t), expected_count);
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
            for i in 0..TestPriorSetImpl::num_prior(t) {
                let cdf = prior_set.get(t, i);
                assert_eq!(cdf.prob, 128u8);
            }
        }

        // Use the priors, updating them by varying degrees.
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(t) {
                let mut cdf = prior_set.get(t, i);
                for j in 0..i {
                    cdf.blend(true);
                }
            }
        }

        // Ascertain that the priors were updated the proper # of times.
        for &t in prior_types.iter() {
            for i in 0..TestPriorSetImpl::num_prior(t) {
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
