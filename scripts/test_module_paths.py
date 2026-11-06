"""Tests of the module-path gate (`module_paths.py`):

    python3 -I -m unittest discover -s scripts -p 'test_*.py'
"""

import textwrap
import unittest

import module_paths


def paths(src: str) -> list[str]:
    return [path for _, path in module_paths.problems(textwrap.dedent(src).lstrip("\n"))]


def supers(src: str) -> list[tuple[int, str]]:
    return module_paths.super_paths(textwrap.dedent(src).lstrip("\n"))


class ModulePaths(unittest.TestCase):
    def test_a_function_through_an_imported_module_of_genie_fails(self):
        src = """
            use genie_math::fmath;
            use crate::gathering::{damage, gather_search};
            fn f(x: f32) -> f32 { fmath::sqrt(x) + damage::of(x) + gather_search::near(x) }
        """
        self.assertEqual(paths(src), ["fmath::sqrt", "damage::of", "gather_search::near"])

    def test_types_constants_std_and_other_crates_pass(self):
        src = """
            use std::{fmt, mem};
            use crate::objects::layers;
            use crate::testing::class;
            use binrw::binrw;
            use genie_math::fmath::sqrt;
            fn f(a: &mut u8) -> layers::Action {
                mem::take(a);
                let _ = (class::GATE, binrw::helpers::until, crate::math::fmath::add);
                sqrt(2.0);
                todo!()
            }
            impl fmt::Display for X { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { Ok(()) } }
        """
        self.assertEqual(paths(src), [])

    def test_a_self_import_and_an_alias_count_and_comments_do_not(self):
        src = """
            use genie_rec::de::{self, Packet};
            use crate::gathering::damage as dmg;
            // de::read(x) in a comment
            fn f() { de::read(1); dmg::of(2); let s = "de::read"; }
        """
        self.assertEqual(paths(src), ["de::read", "dmg::of"])


class SuperPaths(unittest.TestCase):
    def test_a_use_of_super_with_more_than_the_glob_fails(self):
        src = """
            use super::generated::world::{Object, Player};
            use super::super::{LenString16, Positioned};
            use super::Tail as Rest;
            use super::{
                bytes,
                finish,
            };
            use super :: {*, words};
        """
        self.assertEqual(
            supers(src),
            [
                (1, "super::generated::world::{"),
                (2, "super::super::{"),
                (3, "super::Tail"),
                (4, "super::{"),
                (8, "super::{"),
            ],
        )

    def test_a_path_through_super_in_code_fails(self):
        src = """
            fn ends(&self) -> usize {
                self.start + super::written_len(self.player)
            }
        """
        self.assertEqual(supers(src), [(2, "super::written_len")])

    def test_the_glob_of_a_test_module_pub_super_comments_and_strings_pass(self):
        src = """
            pub(super) fn f() {}
            /// Drops them first ([`super::Attack::enter`]).
            #[cfg(test)]
            mod tests {
                use super::*;
                use super :: * ;
                // super::f() in a comment
                fn g() { let s = "super::f"; }
            }
        """
        self.assertEqual(supers(src), [])


if __name__ == "__main__":
    unittest.main()
