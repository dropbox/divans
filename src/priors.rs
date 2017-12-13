// Copyright 2017 Dropbox, Inc
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.

#![allow(unused)]
#![macro_escape]
use core;
use super::probability::{BaseCDF, CDF2, CDF16};
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};

pub trait PriorMultiIndex {
    fn car(&self) -> usize;
    fn cdr(&self) -> (usize, usize, usize, usize);
    fn num_dimensions() -> usize;
}

impl PriorMultiIndex for (usize,) {
    #[inline]
    fn car(&self) -> usize { self.0 }
    #[inline]
    fn cdr(&self) -> (usize, usize, usize, usize) { (1usize, 1usize, 1usize, 1usize) }
    fn num_dimensions() -> usize { 1usize }
}

impl PriorMultiIndex for (usize, usize) {
    #[inline]
    fn car(&self) -> usize { self.0 }
    #[inline]
    fn cdr(&self) -> (usize, usize, usize, usize) { (self.1, 1usize, 1usize, 1usize) }
    fn num_dimensions() -> usize { 2usize }
}

impl PriorMultiIndex for (usize, usize, usize) {
    #[inline]
    fn car(&self) -> usize { self.0 }
    #[inline]
    fn cdr(&self) -> (usize, usize, usize, usize) { (self.1, self.2, 1usize, 1usize) }
    fn num_dimensions() -> usize { 3usize }
}

impl PriorMultiIndex for (usize, usize, usize, usize) {
    #[inline]
    fn car(&self) -> usize { self.0 }
    #[inline]
    fn cdr(&self) -> (usize, usize, usize, usize) { (self.1, self.2, self.3, 1usize) }
    fn num_dimensions() -> usize { 4usize }
}

pub trait PriorCollection<T: BaseCDF + Default, AllocT: Allocator<T>, B: Clone> {
    fn name() -> Option<&'static str> { None }

    const NUM_ALL_PRIORS: usize;
    const NUM_BILLING_TYPES: usize;

    fn initialized(&self) -> bool;
    fn num_prior(billing: &B) -> usize;
    fn num_dimensions(billing: &B) -> usize;
    fn index_to_billing_type(index: usize) -> B;

    fn get<I: PriorMultiIndex>(&mut self, billing: B, index: I) -> &mut T;
    fn get_with_raw_index(&self, billing: B, index: usize) -> &T;
    fn get_with_raw_index_mut(&mut self, billing: B, index: usize) -> &mut T;
}

macro_rules! define_prior_struct {
    // Syntax: define_prior_struct(StructName, BillingType,
    //                             billing_type1, count1, billing_type2, count2, ...);
    // Note that the counts are expressed as tuples, with the least significant dimension
    // being first. So dimensions with more volatile values should come first.
    ($name: ident, $billing_type: ty, $($args:tt),*) => {
        // TODO: this struct should probably own/manage its allocated memory,
        // since it is required to be of a particular size.
        pub struct $name<T: BaseCDF + Default, AllocT: Allocator<T>> {
            pub priors: AllocT::AllocatedMemory
        }
        impl<T: BaseCDF + Default, AllocT: Allocator<T>> $name<T, AllocT> {
            const BILLING_TYPES: [$billing_type; count_expr!($($args),*)] = collect_car_into_array!($($args),*);
        }
        impl<T: BaseCDF + Default, AllocT: Allocator<T>> PriorCollection<T, AllocT, $billing_type> for $name<T, AllocT> {
            const NUM_ALL_PRIORS : usize = sum_product_cdr!($($args),*) as usize;
            const NUM_BILLING_TYPES : usize = count_expr!($($args),*) as usize;

            fn name() -> Option<&'static str> {
                Some(stringify!($name))
            }
            fn initialized(&self) -> bool {
                self.priors.slice().len() == Self::NUM_ALL_PRIORS
            }
            #[inline]
            fn get_with_raw_index(&self, billing: $billing_type, index: usize) -> &T {
                debug_assert!(index < Self::num_prior(&billing), "Offset from the index is out of bounds");
                let offset : usize = define_prior_struct_helper_offset!(billing; $($args),*) as usize;
                debug_assert!(offset + index < Self::NUM_ALL_PRIORS);
                &self.priors.slice()[index + offset]
            }
            #[inline]
            fn get_with_raw_index_mut(&mut self, billing: $billing_type, index: usize) -> &mut T {
                debug_assert!(index < Self::num_prior(&billing), "Offset from the index is out of bounds");
                let offset : usize = define_prior_struct_helper_offset!(billing; $($args),*) as usize;
                debug_assert!(offset + index < Self::NUM_ALL_PRIORS);
                &mut self.priors.slice_mut()[index + offset]
            }
            #[inline]
            fn get<I: PriorMultiIndex>(&mut self, billing: $billing_type, index: I) -> &mut T {
                // Check the dimensionality.
                debug_assert_eq!(I::num_dimensions(), Self::num_dimensions(&billing),
                                 "Index has {} dimensions but {} is expected for {:?}",
                                 I::num_dimensions(), Self::num_dimensions(&billing), billing);
                // Compute the offset arising from the index.
                let linearized_index = define_prior_struct_helper_collapse_index!(&billing; index; $($args),*);
                debug_assert!(linearized_index < Self::NUM_ALL_PRIORS);
                &mut self.priors.slice_mut()[linearized_index]
            }
            // TODO: technically this does not depend on the template paramters.
            fn num_prior(_billing: &$billing_type) -> usize {
                define_prior_struct_unary_helper!(product; _billing; $($args),*) as usize
            }
            fn num_dimensions(_billing: &$billing_type) -> usize {
                define_prior_struct_unary_helper!(count_expr; _billing; $($args),*) as usize
            }
            fn index_to_billing_type(index: usize) -> $billing_type {
                Self::BILLING_TYPES[index].clone()
            }
        }
        #[cfg(all(feature="billing",feature="debug_entropy"))]
        impl<T: BaseCDF + Default, AllocT: Allocator<T>> Drop for $name<T, AllocT> {
            fn drop(&mut self) {
                summarize_prior_billing::<T, AllocT, $billing_type, $name<T, AllocT>>(&self);
            }
        }
    };
}

macro_rules! define_prior_struct_helper_offset {
    ($billing: expr; ($typ: expr, $($args: expr),*)) => {
        { debug_assert_eq!($billing, $typ, "Invalid billing type"); 0 }
    };
    ($billing: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        if $billing == $typ { 0 } else {
            product!($($args),*) + define_prior_struct_helper_offset!($billing; $($more),*)
        }
    };
}

macro_rules! define_prior_struct_unary_helper {
    ($macro: ident; $billing: expr; ($typ: expr, $($args: expr),*)) => { $macro!($($args),*) };
    ($macro: ident; $billing: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        if *$billing != $typ {
            define_prior_struct_unary_helper!($macro; $billing; $($more),*)
        } else {
            $macro!($($args),*)
        }
    };
}

macro_rules! linearize_index {
    ($multi_index: expr; $val: expr) => {
        {
            debug_assert!($multi_index.car() < $val, "Index out of bounds");
            $multi_index.car()
        }
    };
    ($multi_index: expr; $val: expr, $($more: expr),*) => {
        {
            debug_assert!($multi_index.car() < $val, "Index out of bounds");
            $multi_index.car() + ($val) * (linearize_index!($multi_index.cdr(); $($more),*))
        }
    };
}

macro_rules! define_prior_struct_helper_collapse_index {
    ($billing: expr; $multi_index: expr; ($typ: expr, $($args: expr),*)) => {
        linearize_index!($multi_index; $($args),*)
    };
    ($billing: expr; $multi_index: expr; ($typ: expr, $($args: expr),*), $($more:tt),*) => {
        if *$billing == $typ {
            linearize_index!($multi_index; $($args),*)
        } else {
            (product!($($args),*) as usize) + define_prior_struct_helper_collapse_index!($billing; $multi_index; $($more),*)
        }
    };
}

macro_rules! collect_car_into_array {
    ($($x: tt),*) => { [$($x.0),*] }
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

macro_rules! count_expr {
    ($args: expr) => { 1 };
    ($args: expr, $($more: expr),*) => { (1 + count_expr!($($more),*)) };
}

#[cfg(all(feature="billing", feature="debug_entropy"))]
pub fn summarize_prior_billing<T: BaseCDF + Default,
                               AllocT: Allocator<T>,
                               B: core::fmt::Debug + Clone,
                               PriorCollectionImpl: PriorCollection<T, AllocT, B>>(prior_collection: &PriorCollectionImpl) {
    println!("[Summary for {}]", PriorCollectionImpl::name().unwrap_or("Unnamed"));
    if !prior_collection.initialized() {
        return;
    }
    use std::vec::Vec;
    use core::iter::FromIterator;
    for i in 0..PriorCollectionImpl::NUM_BILLING_TYPES {
        let billing = PriorCollectionImpl::index_to_billing_type(i as usize);
        let count = PriorCollectionImpl::num_prior(&billing);
        let mut num_cdfs_printed = 0usize;

        // Sort the bins first by size, then re-sort the top 16 by index.
        const MAX_BINS_PRINTED : usize = 16;
        let mut samples_for_cdf = Vec::from_iter(
            (0..count).into_iter().
                map(|i| (i, prior_collection.get_with_raw_index(billing.clone(), i).num_samples().unwrap_or(0))));
        samples_for_cdf.sort_by_key(|&(_, count)| -(count as i32));

        for j in 0..count {
            if num_cdfs_printed == MAX_BINS_PRINTED {
                println!("  {:?}[...] : omitted", billing);
                break;
            }
            let index = samples_for_cdf[j].0;
            let cdf = prior_collection.get_with_raw_index(billing.clone(), index);
            let true_entropy = cdf.true_entropy();
            let rolling_entropy = cdf.rolling_entropy();
            let num_samples = cdf.num_samples();
            let encoding_cost = cdf.encoding_cost();
            if cdf.used() && true_entropy.is_some() && rolling_entropy.is_some() &&
                num_samples.is_some() && encoding_cost.is_some() {
                    println!("  {:?}[{}] : {:1.5} (Perfect rolling entropy {:1.5}, Final true entropy: {:1.5}), #: {})",
                             billing, index,
                             encoding_cost.unwrap() / (num_samples.unwrap() as f64), // actual encoding cost
                             rolling_entropy.unwrap(), // encoding cost if we kept track of the PDF perfectly
                             true_entropy.unwrap(), // final entropy of the perfect PDF
                             num_samples.unwrap());
                    num_cdfs_printed += 1;
                }
        }
    }
}

mod test {
    use core;
    use probability::{BaseCDF, CDF16, FrequentistCDF16, Speed};
    use super::{PriorCollection, PriorMultiIndex};
    #[cfg(all(feature="billing", feature="debug_entropy"))]
    use super::summarize_prior_billing;
    use alloc::{Allocator, HeapAlloc, SliceWrapper, SliceWrapperMut};

    #[derive(PartialEq, Eq, Clone, Copy, Debug)]
    enum PriorType { Foo, Bar, Cat }
    define_prior_struct!(TestPriorSet, PriorType,
                         (PriorType::Foo, 5, 8, 2), (PriorType::Bar, 6, 2), (PriorType::Cat, 3));

    type TestPriorSetImpl = TestPriorSet<FrequentistCDF16, HeapAlloc<FrequentistCDF16>>;

    #[test]
    fn test_macro_product() {
        assert_eq!(product!(5), 5);
        assert_eq!(product!(2, 3, 4), 2 * 3 * 4);
    }

    #[test]
    fn test_macro_sum_product_cdr() {
        assert_eq!(sum_product_cdr!(("a", 2)), 2);
        assert_eq!(sum_product_cdr!(("a", 2, 3)), 2 * 3);
        assert_eq!(sum_product_cdr!(("a", 2, 3), ("b", 3, 4)), 2 * 3 + 3 * 4);
    }

    #[test]
    fn test_macro_count_expr() {
        assert_eq!(count_expr!(3), 1);
        assert_eq!(count_expr!(3, 4), 2);
        assert_eq!(count_expr!(3, 4, 5), 3);
    }

    #[test]
    fn test_num_prior() {
        let cases : [(PriorType, usize); 3] = [(PriorType::Foo, 5 * 8 * 2),
                                               (PriorType::Bar, 6 * 2),
                                               (PriorType::Cat, 3)];
        let mut expected_sum = 0usize;
        for &(t, expected_count) in cases.iter() {
            assert_eq!(TestPriorSetImpl::num_prior(&t), expected_count);
            expected_sum += expected_count;
        }
        assert_eq!(TestPriorSetImpl::NUM_ALL_PRIORS, expected_sum);
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
        assert_eq!(TestPriorSetImpl::NUM_BILLING_TYPES, 3);
        assert_eq!(TestPriorSetImpl::index_to_billing_type(0), PriorType::Foo);
        assert_eq!(TestPriorSetImpl::index_to_billing_type(1), PriorType::Bar);
        assert_eq!(TestPriorSetImpl::index_to_billing_type(2), PriorType::Cat);
    }

    #[test]
    fn test_get() {
        let mut allocator = HeapAlloc::<FrequentistCDF16>::new(FrequentistCDF16::default());
        let mut prior_set = TestPriorSetImpl {
            priors: allocator.alloc_cell(TestPriorSetImpl::NUM_ALL_PRIORS),
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
                let mut cdf = prior_set.get_with_raw_index_mut(t, i);
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
            priors: allocator.alloc_cell(TestPriorSetImpl::NUM_ALL_PRIORS),
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
            priors: allocator.alloc_cell(TestPriorSetImpl::NUM_ALL_PRIORS),
        };
        prior_set.get(PriorType::Bar, (6, 1));
    }

    #[test]
    #[should_panic]
    fn test_get_bad_tuple_dimensionality() {
        let mut allocator = HeapAlloc::<FrequentistCDF16>::new(FrequentistCDF16::default());
        let mut prior_set = TestPriorSetImpl {
            priors: allocator.alloc_cell(TestPriorSetImpl::NUM_ALL_PRIORS),
        };
        prior_set.get(PriorType::Bar, (0, 0, 0));
    }
}
