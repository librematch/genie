"""Tests of the casing gate (`c_case.py`):

    python3 -I -m unittest discover -s scripts -p 'test_*.py'
"""

import textwrap
import unittest

import c_case


def names(src: str, enums: set[str] | None = None) -> list[str]:
    src = textwrap.dedent(src).lstrip("\n")
    return [name for _, name, _ in c_case.problems(src, enums or c_case.enum_names(src))]


class Types(unittest.TestCase):
    def test_an_acronym_in_capitals_is_one_word(self):
        self.assertEqual(
            names(
                "struct UnitTypeID;\nstruct AIModule;\ntrait DRSReader {}\ntype ObjectID = u8;\n"
            ),
            ["UnitTypeID", "AIModule", "DRSReader", "ObjectID"],
        )

    def test_an_acronym_as_a_word_and_a_single_letter_pass(self):
        self.assertEqual(
            names(
                "struct UnitTypeId;\nstruct AiModule;\nstruct XAxis;\nstruct T;\n"
                "enum Command7a {}\n"
            ),
            [],
        )

    def test_a_variant_is_checked_but_not_its_fields(self):
        src = """
            enum Version {
                #[default]
                HD,
                Aoc { inner: Option<UnitAI> },
                De(u8) = 3,
                AocToWK,
            }
        """
        self.assertEqual(names(src), ["HD", "AocToWK"])


class SnakeNames(unittest.TestCase):
    def test_a_single_letter_is_a_word_only_at_the_end(self):
        src = """
            fn a_build_s_walk() {}
            fn map_x() -> u8 { let t_master = 1; let x = 2; t_master + x }
            const T_HIGH: f32 = 1.0;
            const PI_2: f32 = 2.0;
        """
        self.assertEqual(names(src), ["a_build_s_walk", "t_master", "T_HIGH"])

    def test_a_digit_is_no_letter_and_comments_and_strings_do_not_count(self):
        src = """
            fn state_6_goes_on() {} // a_comment_s_name
            fn x2_y() -> &'static str { "a_string_s_name" }
        """
        self.assertEqual(names(src), [])


class EnumConstants(unittest.TestCase):
    def test_an_enum_holds_no_constant_of_its_own_type(self):
        src = """
            enum State { Move { walk: Option<u8> }, Idle }
            impl State {
                pub const MOVE: Self = Self::Move { walk: None };
                const IDLE: State = State::Idle;
                pub const ALL: [Self; 2] = [Self::Idle, Self::Idle];
            }
        """
        self.assertEqual(names(src), ["State::MOVE", "State::IDLE"])

    def test_a_newtype_keeps_its_constants(self):
        src = """
            struct GameMode(u8);
            impl GameMode {
                pub const REGICIDE: Self = Self(1);
            }
        """
        self.assertEqual(names(src), [])

    def test_an_enum_of_another_crate_does_not_count(self):
        src = "impl GameMode {\n    pub const REGICIDE: Self = Self(1);\n}\n"
        self.assertEqual(names(src, set()), [])


if __name__ == "__main__":
    unittest.main()
