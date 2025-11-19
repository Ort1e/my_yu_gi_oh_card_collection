import re
from django.core.management.base import BaseCommand

from my_ygo_cards.models import Card, CardData


def strip_parentheses(name: str) -> str:
    """Remove parentheses and their content from a string."""
    return re.sub(r"\s*\([^)]*\)", "", name).strip()


class Command(BaseCommand):
    help = "Normalize en_name by removing parentheses content."

    def handle(self, *args, **kwargs):
        updated_carddata = 0
        updated_cards = 0

        # Normalize CardData
        for carddata in CardData.objects.all():
            new_name = strip_parentheses(carddata.en_name)
            new_name = new_name.lower()
            if new_name != carddata.en_name:
                self.stdout.write(f"Updating CardData: {carddata.en_name} -> {new_name}")
                carddata.en_name = new_name
                carddata.save(update_fields=["en_name"])
                updated_carddata += 1

        # Normalize Card
        for card in Card.objects.exclude(en_name__isnull=True):
            if not card.en_name:
                continue
            new_name = strip_parentheses(card.en_name)
            new_name = new_name.lower()
            if new_name != card.en_name:
                self.stdout.write(f"Updating Card: {card.en_name} -> {new_name}")
                card.en_name = new_name
                card.save(update_fields=["en_name"])
                updated_cards += 1

        self.stdout.write(
            self.style.SUCCESS(
                f"Normalization complete. CardData updated: {updated_carddata}, Cards updated: {updated_cards}"
            )
        )
