//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::u32;

use rand;
use rand::distributions::IndependentSample;

use strategy::traits::*;
use test_runner::*;

/// A `Strategy` which picks from one of several delegate `Stragegy`s.
///
/// See `Strategy::prop_union()`.
#[derive(Clone, Debug)]
pub struct Union<T : Strategy> {
    options: Vec<(u32,T)>,
}

impl<T : Strategy> Union<T> {
    /// Create a strategy which selects uniformly from the given delegate
    /// strategies.
    ///
    /// When shrinking, after maximal simplification of the chosen element, the
    /// strategy will move to earlier options and continue simplification with
    /// those.
    ///
    /// ## Panics
    ///
    /// Panics if `options` is empty.
    pub fn new<I : IntoIterator<Item = T>>(options: I) -> Self {
        let options: Vec<(u32,T)> = options.into_iter()
            .map(|v| (1, v)).collect();
        assert!(options.len() > 0);

        Union { options }
    }

    /// Create a strategy which selects from the given delegate strategies.
    ///
    /// Each strategy is assigned a non-zero weight which determines how
    /// frequently that strategy is chosen. For example, a strategy with a
    /// weight of 2 will be chosen twice as frequently as one with a weight of
    /// 1.
    ///
    /// ## Panics
    ///
    /// Panics if `options` is empty or any element has a weight of 0.
    ///
    /// Panics if the sum of the weights overflows a `u32`.
    pub fn new_weighted(options: Vec<(u32,T)>) -> Self {
        assert!(options.len() > 0);
        assert!(!options.iter().any(|&(w, _)| 0 == w),
                "Union option has a weight of 0");
        assert!(options.iter().map(|&(w, _)| w as u64).sum::<u64>() <=
                u32::MAX as u64, "Union weights overflow u32");
        Union { options }
    }

    /// Add `other` as an additional alternate strategy with weight 1.
    pub fn or(mut self, other: T) -> Self {
        self.options.push((1, other));
        self
    }
}

fn pick_weighted<I : Iterator<Item = u32>>(runner: &mut TestRunner,
                                           weights1: I, weights2: I) -> usize {
    let sum = weights1.sum();
    let weighted_pick = rand::distributions::Range::new(0, sum)
        .ind_sample(runner.rng());
    weights2.scan(0, |state, w| {
        *state += w;
        Some(*state)
    }).filter(|&v| v <= weighted_pick).count()
}

impl<T : Strategy> Strategy for Union<T> {
    type Value = UnionValueTree<T::Value>;

    fn new_value(&self, runner: &mut TestRunner)
                 -> Result<Self::Value, String> {
        fn extract_weight<V>(&(w, _): &(u32, V)) -> u32 { w }

        let pick = pick_weighted(
            runner,
            self.options.iter().map(extract_weight::<T>),
            self.options.iter().map(extract_weight::<T>));

        let mut options = Vec::with_capacity(pick);
        for option in &self.options[0..pick+1] {
            options.push(option.1.new_value(runner)?);
        }

        Ok(UnionValueTree {
            options: options,
            pick: pick,
            min_pick: 0,
            prev_pick: None,
        })
    }
}

/// `ValueTree` corresponding to `Union`.
#[derive(Clone, Debug)]
pub struct UnionValueTree<T : ValueTree> {
    options: Vec<T>,
    pick: usize,
    min_pick: usize,
    prev_pick: Option<usize>,
}

macro_rules! access_vec {
    ([$($muta:tt)*] $dst:ident = $this:expr, $ix:expr, $body:block) => {{
        let $dst = &$($muta)* $this.options[$ix];
        $body
    }}
}

macro_rules! union_value_tree_body {
    ($typ:ty, $access:ident) => {
        type Value = $typ;

        fn current(&self) -> Self::Value {
            $access!([] opt = self, self.pick, {
                opt.current()
            })
        }

        fn simplify(&mut self) -> bool {
            if $access!([mut] opt = self, self.pick, { opt.simplify() }) {
                self.prev_pick = None;
                true
            } else if self.pick > self.min_pick {
                self.prev_pick = Some(self.pick);
                self.pick -= 1;
                true
            } else {
                false
            }
        }

        fn complicate(&mut self) -> bool {
            if let Some(pick) = self.prev_pick {
                self.pick = pick;
                self.min_pick = pick;
                self.prev_pick = None;
                true
            } else {
                $access!([mut] opt = self, self.pick, { opt.complicate() })
            }
        }
    }
}

impl<T : ValueTree> ValueTree for UnionValueTree<T> {
    union_value_tree_body!(T::Value, access_vec);
}

macro_rules! def_access_tuple {
    ($b:tt $name:ident, $($n:tt)*) => {
        macro_rules! $name {
            ([$b($b muta:tt)*] $b dst:ident = $b this:expr,
             $b ix:expr, $b body:block) => {
                match $b ix {
                    0 => {
                        let $b dst = &$b($b muta)* $b this.options.0;
                        $b body
                    },
                    $(
                        $n => {
                            if let Some(ref $b($b muta)* $b dst) =
                                $b this.options.$n
                            {
                                $b body
                            } else {
                                unreachable!()
                            }
                        },
                    )*
                    _ => unreachable!() }
            }
        }
    }
}

def_access_tuple!($ access_tuple2, 1);
def_access_tuple!($ access_tuple3, 1 2);
def_access_tuple!($ access_tuple4, 1 2 3);
def_access_tuple!($ access_tuple5, 1 2 3 4);
def_access_tuple!($ access_tuple6, 1 2 3 4 5);
def_access_tuple!($ access_tuple7, 1 2 3 4 5 6);
def_access_tuple!($ access_tuple8, 1 2 3 4 5 6 7);
def_access_tuple!($ access_tuple9, 1 2 3 4 5 6 7 8);
def_access_tuple!($ access_tupleA, 1 2 3 4 5 6 7 8 9);

/// Similar to `Union`, but internally uses a tuple to hold the strategies.
///
/// This allows better performance than vanilla `Union` since one does not need
/// to resort to boxing and dynamic dispatch to handle heterogeneous
/// strategies.
#[derive(Clone, Copy, Debug)]
pub struct TupleUnion<T>(T);

impl<T> TupleUnion<T> {
    /// Wrap `tuple` in a `TupleUnion`.
    ///
    /// The struct definition allows any `T` for `tuple`, but to be useful, it
    /// must be a 2- to 10-tuple of `(u32, impl Strategy)` pairs where all
    /// strategies ultimately produce the same value. Each `u32` indicates the
    /// relative weight of its corresponding strategy.
    ///
    /// Using this constructor directly is discouraged; prefer to use
    /// `prop_oneof!` since it is generally clearer.
    pub fn new(tuple: T) -> Self {
        TupleUnion(tuple)
    }
}

macro_rules! tuple_union {
    ($($gen:ident $ix:tt)*) => {
        impl<A : Strategy, $($gen: Strategy),*> Strategy
        for TupleUnion<((u32, A), $((u32, $gen)),*)>
        where $($gen::Value : ValueTree<Value =
                <A::Value as ValueTree>::Value>),* {
            type Value = TupleUnionValueTree<
                (A::Value, $(Option<$gen::Value>),*)>;

            fn new_value(&self, runner: &mut TestRunner)
                         -> Result<Self::Value, String> {
                let weights = [((self.0).0).0, $(((self.0).$ix).0),*];
                let pick = pick_weighted(runner, weights.iter().cloned(),
                                         weights.iter().cloned());

                Ok(TupleUnionValueTree {
                    options: (
                        ((self.0).0).1.new_value(runner)?,
                        $(if pick <= $ix {
                            Some(((self.0).$ix).1.new_value(runner)?)
                        } else {
                            None
                        }),*),
                    pick: pick,
                    min_pick: 0,
                    prev_pick: None,
                })
            }
        }
    }
}

tuple_union!(B 1);
tuple_union!(B 1 C 2);
tuple_union!(B 1 C 2 D 3);
tuple_union!(B 1 C 2 D 3 E 4);
tuple_union!(B 1 C 2 D 3 E 4 F 5);
tuple_union!(B 1 C 2 D 3 E 4 F 5 G 6);
tuple_union!(B 1 C 2 D 3 E 4 F 5 G 6 H 7);
tuple_union!(B 1 C 2 D 3 E 4 F 5 G 6 H 7 I 8);
tuple_union!(B 1 C 2 D 3 E 4 F 5 G 6 H 7 I 8 J 9);

/// `ValueTree` type produced by `TupleUnion`.
#[derive(Clone, Copy, Debug)]
pub struct TupleUnionValueTree<T> {
    options: T,
    pick: usize,
    min_pick: usize,
    prev_pick: Option<usize>,
}

macro_rules! value_tree_tuple {
    ($access:ident, $($gen:ident)*) => {
        impl<A : ValueTree, $($gen: ValueTree<Value = A::Value>),*> ValueTree
        for TupleUnionValueTree<(A, $(Option<$gen>),*)> {
            union_value_tree_body!(A::Value, $access);
        }
    }
}

value_tree_tuple!(access_tuple2, B);
value_tree_tuple!(access_tuple3, B C);
value_tree_tuple!(access_tuple4, B C D);
value_tree_tuple!(access_tuple5, B C D E);
value_tree_tuple!(access_tuple6, B C D E F);
value_tree_tuple!(access_tuple7, B C D E F G);
value_tree_tuple!(access_tuple8, B C D E F G H);
value_tree_tuple!(access_tuple9, B C D E F G H I);
value_tree_tuple!(access_tupleA, B C D E F G H I J);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_union() {
        let input = (10u32..20u32).prop_union(30u32..40u32);
        // Expect that 25% of cases pass (left input happens to be < 15, and
        // left is chosen as initial value). Of the 75% that fail, 50% should
        // converge to 15 and 50% to 30 (the latter because the left is beneath
        // the passing threshold).
        let mut passed = 0;
        let mut converged_low = 0;
        let mut converged_high = 0;
        for _ in 0..256 {
            let mut runner = TestRunner::new(Config::default());
            let case = input.new_value(&mut runner).unwrap();
            let result = runner.run_one(case, |&v| if v < 15 {
                Ok(())
            } else {
                Err(TestCaseError::Fail("fail".to_owned()))
            });

            match result {
                Ok(true) => passed += 1,
                Err(TestError::Fail(_, 15)) => converged_low += 1,
                Err(TestError::Fail(_, 30)) => converged_high += 1,
                e => panic!("Unexpected result: {:?}", e),
            }
        }

        assert!(passed >= 32 && passed <= 96,
                "Bad passed count: {}", passed);
        assert!(converged_low >= 32 && converged_low <= 160,
                "Bad converged_low count: {}", converged_low);
        assert!(converged_high >= 32 && converged_high <= 160,
                "Bad converged_high count: {}", converged_high);
    }

    #[test]
    fn test_union_weighted() {
        let input = Union::new_weighted(vec![
            (1, Just(0usize)),
            (2, Just(1usize)),
            (1, Just(2usize)),
        ]);

        let mut counts = [0, 0, 0];
        let mut runner = TestRunner::new(Config::default());
        for _ in 0..65536 {
            counts[input.new_value(&mut runner).unwrap().current()] += 1;
        }

        println!("{:?}", counts);
        assert!(counts[0] > 0);
        assert!(counts[2] > 0);
        assert!(counts[1] > counts[0] * 3/2);
        assert!(counts[1] > counts[2] * 3/2);
    }

    #[test]
    fn test_tuple_union() {
        let input = TupleUnion::new(
            ((1, 10u32..20u32),
             (1, 30u32..40u32)));
        // Expect that 25% of cases pass (left input happens to be < 15, and
        // left is chosen as initial value). Of the 75% that fail, 50% should
        // converge to 15 and 50% to 30 (the latter because the left is beneath
        // the passing threshold).
        let mut passed = 0;
        let mut converged_low = 0;
        let mut converged_high = 0;
        for _ in 0..256 {
            let mut runner = TestRunner::new(Config::default());
            let case = input.new_value(&mut runner).unwrap();
            let result = runner.run_one(case, |&v| if v < 15 {
                Ok(())
            } else {
                Err(TestCaseError::Fail("fail".to_owned()))
            });

            match result {
                Ok(true) => passed += 1,
                Err(TestError::Fail(_, 15)) => converged_low += 1,
                Err(TestError::Fail(_, 30)) => converged_high += 1,
                e => panic!("Unexpected result: {:?}", e),
            }
        }

        assert!(passed >= 32 && passed <= 96,
                "Bad passed count: {}", passed);
        assert!(converged_low >= 32 && converged_low <= 160,
                "Bad converged_low count: {}", converged_low);
        assert!(converged_high >= 32 && converged_high <= 160,
                "Bad converged_high count: {}", converged_high);
    }

    #[test]
    fn test_tuple_union_weighting() {
        let input = TupleUnion::new((
            (1, Just(0usize)),
            (2, Just(1usize)),
            (1, Just(2usize)),
        ));

        let mut counts = [0, 0, 0];
        let mut runner = TestRunner::new(Config::default());
        for _ in 0..65536 {
            counts[input.new_value(&mut runner).unwrap().current()] += 1;
        }

        println!("{:?}", counts);
        assert!(counts[0] > 0);
        assert!(counts[2] > 0);
        assert!(counts[1] > counts[0] * 3/2);
        assert!(counts[1] > counts[2] * 3/2);
    }

    #[test]
    fn test_tuple_union_all_sizes() {
        let mut runner = TestRunner::new(Config::default());
        let r = 1i32..10;

        macro_rules! test {
            ($($part:expr),*) => {{
                let input = TupleUnion::new((
                    $((1, $part.clone())),*,
                    (1, Just(0i32))
                ));

                let mut pass = false;
                for _ in 0..1024 {
                    if 0 == input.new_value(&mut runner).unwrap().current() {
                        pass = true;
                        break;
                    }
                }

                assert!(pass);
            }}
        }

        test!(r); // 2
        test!(r, r); // 3
        test!(r, r, r); // 4
        test!(r, r, r, r); // 5
        test!(r, r, r, r, r); // 6
        test!(r, r, r, r, r, r); // 7
        test!(r, r, r, r, r, r, r); // 8
        test!(r, r, r, r, r, r, r, r); // 9
        test!(r, r, r, r, r, r, r, r, r); // 10
    }
}
