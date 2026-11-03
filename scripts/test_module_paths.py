"""Tests of the module-path gate (`module_paths.py`):

    python3 -I -m unittest discover -s scripts -p 'test_*.py'
"""

import textwrap
import unittest

import module_paths


def paths(src: str) -> list[str]:
    return [path for _, path in module_paths.problems(textwrap.dedent(src).lstrip("\n"))]


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


if __name__ == "__main__":
    unittest.main()
