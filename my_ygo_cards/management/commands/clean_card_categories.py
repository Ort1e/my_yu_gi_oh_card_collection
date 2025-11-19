from django.core.management.base import BaseCommand
from django.db.models import Q, F

from my_ygo_cards.models import CardCategoryAssignment


class Command(BaseCommand):
    help = "Remove category assignments for cards that are no longer in their DeckVersion."

    def handle(self, *args, **kwargs):
        self.stdout.write("🔍 Scanning for invalid CardCategoryAssignment entries...")

        # A category assignment is valid only if the card is still in any deck zone.
        # We delete all assignments where this is not true.
        invalid_assignments = CardCategoryAssignment.objects.exclude(
            card__main_decks=F("category__deck_version")
        ).exclude(
            card__extra_decks=F("category__deck_version")
        ).exclude(
            card__side_decks=F("category__deck_version")
        )

        count = invalid_assignments.count()

        if count > 0:
            invalid_assignments.delete()
            self.stdout.write(self.style.SUCCESS(f"✅ Removed {count} invalid assignments."))
        else:
            self.stdout.write(self.style.SUCCESS("✅ No invalid assignments found."))
