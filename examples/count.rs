// Copyright (C) 2017 Steve Sprang
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! Finds all n-card deals that contain no SuperSets.
//!
//! The [description of SuperSet](http://magliery.com/Set/SuperSet.html) indicates that the
//! odds are good that a 9-card deal will contain a SuperSet. It's known that the smallest deal
//! guaranteed to contain a Set is 21 cards. What is the smallest deal guaranteed to contain a
//! SuperSet?
//!
//! Results on *AMD Ryzen 7 1800x @ 3.9GHz* with 16 threads:
//!
//!  deal |         supersets | no supersets |             total |  % without |            time
//! ------+-------------------+--------------+-------------------+------------+-----------------
//!     4 |            63_180 |    1_600_560 |         1_663_740 | 96.20253 % |  PT0.004201565S
//!     5 |         4_696_380 |   20_925_216 |        25_621_596 | 81.67023 % |  PT0.058930473S
//!     6 |       155_521_080 |  169_019_136 |       324_540_216 | 52.07957 % |  PT0.759225487S
//!     7 |     2_808_519_480 |  668_697_120 |     3_477_216_600 | 19.23082 % |  PT8.129868621S
//!     8 |    31_413_675_150 |  750_578_400 |    32_164_253_550 |  2.33358 % | PT41.347639338S
//!     9 |   260_868_122_190 |   19_712_160 |   260_887_834_350 |  0.00756 % | PT75.742618054S
//!    10 | 1_878_392_407_320 |            0 | 1_878_392_407_320 |  0.00000 % | PT66.465437165S
//!
//! Donald Knuth wrote two very efficient programs that find all the deals that contain no Sets
//! (SETSET and SETSET-ALL here: <https://cs.stanford.edu/~uno/programs.html>). At some point
//! I'd like to study these programs and apply the same techniques here.
//!
//! As it is, this program runs in about 3 minutes on my machine. It makes use of the fact that
//! there is an isomorphism between a `core::Card` and its index. It only uses `core::Card`
//! objects directly when initializing the `SETS` lookup table, and otherwise just works with
//! the cards by index. It recursively builds up a hand of cards, and abandons branches of the
//! search tree as soon as the hand contains a SuperSet.
//!
//! As implemented, we have to count each deal size explicitly. We will undercount if we also
//! count smaller deals as we are counting a larger deal size. By abandoning branches of the
//! search tree as soon as a SuperSet is found, we don't reach every sub-deal that might be
//! SuperSet-free.
//!

#![allow(unknown_lints)]
#![allow(needless_range_loop)]

#[macro_use] extern crate clap;
extern crate core;
extern crate num_cpus;
#[macro_use] extern crate prettytable;
extern crate time;

use prettytable::Table;
use prettytable::format::consts;
use std::ops::Range;
use std::sync::mpsc;
use std::{cmp, thread};
use time::{Duration, PreciseTime};

use core::card::*;
use core::deck::cards;
use core::pair_iter::PairIter;
use core::utils::pretty_print;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The number of cards composing a SuperSet.
const SUPERSET_SIZE: usize = 4;

struct Combination {
    /// Cards available to combine. `usize` stands in for `core::Card` here.
    deck: Vec<usize>,
    /// Current combination.
    hand: Vec<usize>,
    /// Number of times we've dealt N cards and found no SuperSets.
    null_count: u64,
}

struct Count {
    /// Stuck hands.
    no_supersets: u64,
    /// Total possible combinations.
    combinations: u64,
    /// Duration of computation.
    time: Duration
}

fn count_null_supersets(deal_size: usize, threads: usize) -> Count {
    assert!(threads > 0);
    assert!(deal_size >= SUPERSET_SIZE);

    // initialize lookup table
    build_lookup();

    // launch threads
    let start_time = PreciseTime::now();
    let start_index = deal_size - 1;
    let (tx, rx) = mpsc::channel();

    for i in start_index..(start_index + threads) {
        let tx = tx.clone();
        thread::spawn(move || {
            let zeroes = deal_hands(i, threads, deal_size);
            tx.send(zeroes).unwrap();
        });
    }

    // collate results
    let mut sum = 0;
    for _ in 0..threads {
        sum += rx.recv().unwrap();
    }

    Count {
        no_supersets: sum,
        combinations: choose(81, deal_size as u64),
        time: start_time.to(PreciseTime::now()),
    }
}

fn deal_hands(start: usize, step: usize, deal_size: usize) -> u64 {
    // our deck of cards is really a deck of card indices
    let cards = (0..81).collect::<Vec<usize>>();

    // start, start + step, start + 2*step, ...
    let iter = (0..)
        .map(|x| start + x * step)
        .take_while(|&x| x < 81);

    let mut data = Combination {
        deck: cards,
        hand: Vec::with_capacity(deal_size),
        null_count: 0
    };

    for x in iter {
        data.hand.push(data.deck[x]);
        deal_another_card(&mut data, (deal_size - 2)..x);
        data.hand.pop();
    }

    data.null_count
}

fn deal_another_card(data: &mut Combination, range: Range<usize>) {
    let depth = range.start;

    for y in range {
        let next_card = data.deck[y];

        if data.hand.len() >= (SUPERSET_SIZE-1) && contains_superset(&data.hand, next_card) {
            // There's already at least one SuperSet, so we can skip this branch
            continue;
        }

        if depth == 0 {
            // The hand is full and it doesn't contain a SuperSet
            data.null_count += 1;
        } else {
            // recursively add another card
            data.hand.push(next_card);
            deal_another_card(data, (depth - 1)..y);
            data.hand.pop();
        }
    }
}

fn generate_table(num_threads: usize) {
    let mut table = Table::new();
    table.set_format(*consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row![r => "deal", "supersets", "no supersets", "total", "% without", "time"]);

    for deal in 4.. {
        let count = count_null_supersets(deal, num_threads);

        // calculate derivable stats
        let sets = count.combinations - count.no_supersets;
        let percentage = (count.no_supersets as f64 / count.combinations as f64) * 100.;

        table.add_row(row![r => &deal.to_string(),
                           &pretty_print(sets),
                           &pretty_print(count.no_supersets),
                           &pretty_print(count.combinations),
                           &format!("{:.5} %", percentage),
                           &count.time.to_string()]);
        table.printstd();
        println!();

        if count.no_supersets == 0 {
            break;
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// main
////////////////////////////////////////////////////////////////////////////////

fn main() {
    let matches = clap_app!(count =>
        (version: VERSION)
        (about: "Finds all n-card deals that contain no SuperSets.")
        (@arg THREADS: -t --threads +takes_value "Sets number of threads")
    ).get_matches();

    let num_threads = value_t!(matches, "THREADS", usize).unwrap_or(num_cpus::get());

    println!("Finding all n-card deals that contain no SuperSets.");
    println!("This could take some time...\n");
    generate_table(num_threads);
}

////////////////////////////////////////////////////////////////////////////////
// Support Functions
////////////////////////////////////////////////////////////////////////////////

/// Computes the binomial coefficient (n k). This function overflows
/// for (81 k) where 18 < k < 63. Could use `BigUint`, but this is
/// sufficient for the values needed here.
///
/// https://en.wikipedia.org/wiki/Binomial_coefficient
fn choose(n: u64, k: u64) -> u64 {
    let m = cmp::min(k, n - k) + 1;
    (1..m).fold(1, |product, i| product * (n + 1 - i) / i)
}

/// Lookup table for Sets.
static mut SETS: [[usize; 81]; 81] = [[0; 81]; 81];

fn build_lookup() {
    let cards = cards();

    for (&a, &b) in (0..81).collect::<Vec<_>>().pairs() {
        let c = (cards[a], cards[b]).complete_set().index();
        unsafe {
            SETS[a][b] = c;
            // `complete_set()` is commutative
            SETS[b][a] = c;
        }
    }
}

/// Make nested unchecked accesses less clunky.
macro_rules! lookup {
    ($a:ident, $b:ident) => {
        SETS.get_unchecked($a).get_unchecked($b);
    }
}

fn is_superset(a: usize, b: usize, c: usize, d: usize) -> bool {
    unsafe {
        lookup!(a, b) == lookup!(c, d)
            || lookup!(a, c) == lookup!(b, d)
            || lookup!(a, d) == lookup!(b, c)
    }
}

/// This function assumes that `hand` does not already contain a
/// SuperSet. It only tests combinations that include `extra`.
fn contains_superset(hand: &[usize], extra: usize) -> bool {
    for a in 2..hand.len() {
        for b in 1..a {
            for c in 0..b {
                if is_superset(hand[a], hand[b], hand[c], extra) {
                    return true;
                }
            }
        }
    }

    false
}
