from django.test import TestCase

from ..models import DeckVersion

EXODIA_DECK_YDKE = "ydke://ryPeAK8j3gCvI94A1Jj9ASoL0gXBMZ8DwTGfA6y8awSsvGsEiTk/A4k5PwOJOT8DXoC3ASqVZQEqlWUBb6pPAm+qTwJvqk8Cs8MRALPDEQCzwxEAjZR4AHBQpwJS5zkE+fl7AD6kcQE+pHEBRQTgBV/wPQJf8D0CX/A9Al1p3gJdad4CXWneApiRwQOYkcEDFe6rAtxgaAHcYGgBZ7n1BQ==!+0rlBCisxwJIiLMAfolDA7VMKwQmGXYDamj2BGpo9gRqaPYEMpglAHvs+AKkmisAt95HBPZQFwC6TtkF!!"

CARDS_TO_TEST_MAIN = [
    "Ash Blossom & Joyous Spring",
    "Right Leg of the Forbidden One",
    "Exodia the Forbidden One",

    "Wedju Temple",
    "Angel Statue - Azurune"
]

CARDS_TO_TEST_EXTRA = [
    "Fiendsmith's Desirae",
    "Garura, Wings of Resonant Life",
    "The Unstoppable Exodia Incarnate"
]

class ImportYDKETest(TestCase):
    def test_cardmarket_shipment(self):
        deck = DeckVersion.from_ydke(None, EXODIA_DECK_YDKE, name="Exodia Deck")
        self.assertIsInstance(deck, DeckVersion)

        self.assertEqual(deck.version_name, "Exodia Deck")

        self.assertEqual(deck.main_deck.count(), 40)
        self.assertEqual(deck.extra_deck.count(), 15)
        self.assertEqual(deck.side_deck.count(), 0)

        # test if all the card are proxies
        for card in deck.main_deck.all():
            self.assertTrue(card.is_proxy)
        
        for card in deck.extra_deck.all():
            self.assertTrue(card.is_proxy)


        # test the presences of some cards
        for card_name in CARDS_TO_TEST_MAIN:
            self.assertTrue(deck.main_deck.filter(name=card_name.lower()).exists(), f"Card '{card_name}' not found in main deck")
        
        for card_name in CARDS_TO_TEST_EXTRA:
            self.assertTrue(deck.extra_deck.filter(name=card_name.lower()).exists(), f"Card '{card_name}' not found in extra deck")