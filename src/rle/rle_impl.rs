use std::ops::Range;

use num::Integer;
use num::NumCast;
use smallvec::{Array, SmallVec};

use super::{HasIndex, HasLength, Mergable, Sliceable};

impl Sliceable for bool {
  fn slice(&self, _: usize, _: usize) -> Self {
    *self
  }
}

impl<T: Integer + NumCast + Copy> Sliceable for Range<T> {
  fn slice(&self, start: usize, end: usize) -> Self {
    let start_off = NumCast::from(start).unwrap();
    let end_off = NumCast::from(end).unwrap();
    self.start + start_off..self.start + end_off
  }
}

impl<T: PartialOrd<T> + Copy> Mergable for Range<T> {
  fn is_mergable(&self, other: &Self, _: &()) -> bool {
    other.start <= self.end && other.start >= self.start
  }

  fn merge(&mut self, other: &Self, _conf: &()) {
    self.end = other.end;
  }
}

impl<T: Integer + NumCast + Copy> HasLength for Range<T> {
  fn content_len(&self) -> usize {
    NumCast::from(self.end - self.start).unwrap()
  }
}

impl<T: super::GlobalIndex + NumCast> HasIndex for Range<T> {
  type Int = T;

  fn get_start_index(&self) -> Self::Int {
    self.start
  }
}

/// This can make iter return type have len.
impl<A, T: HasLength> HasLength for (A, T) {
  fn content_len(&self) -> usize {
    self.1.content_len()
  }
}

/// This can make iter return type have len.
impl<T: HasLength> HasLength for &T {
  fn content_len(&self) -> usize {
    (*self).content_len()
  }
}

impl<T: HasLength + Sliceable, A: Array<Item = T>> Sliceable for SmallVec<A> {
  fn slice(&self, from: usize, to: usize) -> Self {
    let mut index = 0;
    let mut ans = SmallVec::new();
    if to == from {
      return ans;
    }

    for item in self.iter() {
      if index < to && from < index + item.atom_len() {
        let start = from.saturating_sub(index);
        ans.push(item.slice(start, item.atom_len().min(to - index)));
      }

      index += item.atom_len();
    }

    ans
  }
}
