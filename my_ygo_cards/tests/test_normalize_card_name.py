from django.test import TestCase

from my_ygo_cards.models.card import normalize_card_name


class NormalizeCardNameTests(TestCase):

    def test_basic_normalization(self):
        self.assertEqual(
            normalize_card_name("  Dark Magician  "),
            "dark magician"
        )

    def test_parentheses_are_removed(self):
        self.assertEqual(
            normalize_card_name("Blue-Eyes White Dragon (Limited Edition)"),
            "blue-eyes white dragon"
        )

    def test_maliss_q_is_removed(self):
        self.assertEqual(
            normalize_card_name("Maliss q Alpha"),
            "maliss alpha"
        )

    def test_maliss_p_is_removed(self):
        self.assertEqual(
            normalize_card_name("Maliss p Beta"),
            "maliss beta"
        )

    def test_non_maliss_q_is_not_removed(self):
        self.assertEqual(
            normalize_card_name("Other q Name"),
            "other q name"
        )

    def test_maliss_without_spaces_is_untouched(self):
        self.assertEqual(
            normalize_card_name("Malissq Alpha"),
            "malissq alpha"
        )

    def test_combined_normalization(self):
        self.assertEqual(
            normalize_card_name("  Maliss q Gamma (Promo) "),
            "maliss gamma"
        )

    def test_c_removal(self):
        self.assertEqual(
            normalize_card_name("  Maliss c Gamma (Promo) "),
            "maliss gamma"
        )