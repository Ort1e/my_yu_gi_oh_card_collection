from dataclasses import dataclass
import random
from collections import defaultdict
from django.db.models import Prefetch
from my_ygo_cards.models import CardCategory, CardCategoryAssignment, DeckVersion
from my_ygo_cards.models.card import Card
from my_ygo_cards.models.category import CardConditionalCategory

@dataclass
class MonteCarloCategoryResult:
    category_id: int
    name: str
    occurences: int

    def __init__(self, category_id: int, name: str):
        self.category_id = category_id
        self.name = name
        self.occurences = 0

    def add_occurrence(self, count: int):
        self.occurences += count

    def to_dict(self):
        return {
            "category_id": self.category_id,
            "name": self.name,
            "occurences": self.occurences,
        }
    
@dataclass
class MonteCarlosDetailsResultsPerCategory:
    category_id: int
    name: str
    # nb_in_main → nb_occurences
    nb_per_main_occurences: dict[int, int]

    def __init__(self, category_id: int, name: str):
        self.category_id = category_id
        self.name = name
        self.nb_per_main_occurences = defaultdict(int)

    def add_occurrence(self, nb_in_main: int):
        self.nb_per_main_occurences[nb_in_main] += 1
    
    def to_dict(self):
        return {
            "category_id": self.category_id,
            "name": self.name,
            "nb_per_main_occurences": dict(self.nb_per_main_occurences),
        }

def run_monte_carlo_simulation(deck_version: DeckVersion, nb_cards_by_simulation: int, nb_simulation: int):
    """
    Optimized Monte Carlo simulation on a deck.
    """

    # Cache queries once to avoid repeated DB hits
    main_deck = list(deck_version.main_deck.all())
    categories_qs = deck_version.categories.prefetch_related( # type: ignore
        Prefetch(
            'cardconditionalcategory__categories_or_conditions',
            to_attr='prefetched_conditions'
        )
    )

    categories_dict: dict[int, CardCategory] = {cat.pk: cat for cat in categories_qs}
    category_assignments = CardCategoryAssignment.objects.filter(
        category__deck_version=deck_version
    ).select_related("category", "card")

    # Build a fast lookup: card.id → [category id]
    card_to_categories_id = defaultdict(list[int])
    for assign in category_assignments:
        card_to_categories_id[assign.card.pk].append(assign.category.pk)

    # Initialize counters
    category_stats = {cat.pk : MonteCarloCategoryResult(cat.pk, cat.name) for cat in categories_dict.values()}
    category_details_stats = {cat.pk : MonteCarlosDetailsResultsPerCategory(cat.pk, cat.name) for cat in categories_dict.values()}

    deck_size = len(main_deck)
    draws = min(nb_cards_by_simulation, deck_size)

    for _ in range(nb_simulation):
        # random.sample is faster than shuffling + slicing
        drawn_cards: list[Card] = random.sample(main_deck, draws)
        categories_nb_in_main = defaultdict(int)
        for cat in categories_dict.values():
            categories_nb_in_main[cat.pk] = 0

        for card in drawn_cards:
            other_card_categories_id : list[CardCategory] = list()
            for cat_id in card_to_categories_id.get(card.pk, []):
                category_stats[cat_id].add_occurrence(1)
                categories_nb_in_main[cat_id] += 1

                # Handle conditional categories
                # store all categories the other cards belong to
                category = categories_dict[cat_id]
                if hasattr(category, 'cardconditionalcategory'):
                    conditional_category: CardConditionalCategory = category.cardconditionalcategory # type: ignore
            
                    if len(other_card_categories_id) == 0:
                        tmp = set()
                        for other_card in drawn_cards:
                            if other_card.pk != card.pk:
                                tmp.update([categories_dict[cat_id] for cat_id in card_to_categories_id.get(other_card.pk, [])])

                        other_card_categories_id = list(tmp)
                    evaluated_category = conditional_category.evaluate_for_categories(list(other_card_categories_id))
                    
                    if evaluated_category:
                        category_stats[evaluated_category.pk].add_occurrence(1)
                        categories_nb_in_main[evaluated_category.pk] += 1

            
            
        for cat_id, count in categories_nb_in_main.items():
            category_details_stats[cat_id].add_occurrence(count)

    return {
        "total_simulations": nb_simulation,
        "category_stats": [
            res.to_dict()
            for res in category_stats.values()
        ],
        "category_details_stats": [
            res.to_dict()
            for res in category_details_stats.values()
        ],
    }
