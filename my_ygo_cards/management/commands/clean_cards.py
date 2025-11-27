import re
from django.core.management.base import BaseCommand

from my_ygo_cards.models import Card, CardData


def strip_parentheses(name: str) -> str:
    """Remove parentheses and their content from a string."""
    return re.sub(r"\s*\([^)]*\)", "", name).strip()


class Command(BaseCommand):
    help = "Normalize Card and CardData names, update CardData references, and clean up proxy cards."

    def handle(self, *args, **kwargs):
        updated_carddata = 0
        updated_cards = 0
        proxy_removed = 0

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
        for card in Card.objects.all():
            if not card.en_name:
                self.stdout.write(
                    self.style.WARNING(f"Skipping Card with empty en_name: ID {card.pk}")
                )
                continue
            has_updated = False
            new_name = strip_parentheses(card.en_name)
            new_name = new_name.lower()
            
            if new_name != card.en_name:
                self.stdout.write(f"Updating Card: {card.en_name} -> {new_name}")
                card.en_name = new_name
                card.save(update_fields=["en_name"])
                card.refresh_from_db(fields=["en_name"])
                has_updated = True

            # Find or fetch correct CardData
            correct_data = CardData.get_or_fetch(card.en_name)
            if correct_data is None:
                self.stdout.write(
                    self.style.ERROR(
                        f"  ERROR: Could not find or fetch CardData for {card.en_name}"
                    )
                )

            if card.card_data != correct_data:
                str_card_data = (
                    card.card_data.en_name if card.card_data else "None"
                )
                self.stdout.write(
                    f"Updating CardData for Card: {card.en_name} ({str_card_data})"
                )
                if not correct_data:
                    self.stdout.write(
                        self.style.ERROR(
                            f"  ERROR: Could not find or fetch CardData for {card.en_name}"
                        )
                    )
                    continue
                card.card_data = correct_data
                card.save(update_fields=["card_data"])
                card.refresh_from_db(fields=["card_data"])
                has_updated = True
            
            if has_updated:
                updated_cards += 1

        # clean proxy :
        for card in Card.objects.exclude(en_name__isnull=True):
            if card.is_proxy:
                # 1) Find all deck versions where this proxy is used
                deck_versions = card.deck_versions

                if deck_versions.exists():
                    for dv in deck_versions:

                        # 2) Find all *physical* non-proxy alternatives with same card_data
                        alternatives = Card.objects.filter(
                            card_data=card.card_data,
                            is_proxy=False
                        ).distinct()

                        # If no real card exists for this card_data: nothing to replace
                        if not alternatives.exists():
                            continue

                        # 3) Determine which cards are already used in this deck version
                        already_used_ids = set(
                            list(dv.main_deck.values_list("id", flat=True)) +
                            list(dv.extra_deck.values_list("id", flat=True)) +
                            list(dv.side_deck.values_list("id", flat=True))
                        )

                        # 4) Find a real card NOT present in this deck yet
                        replacement = alternatives.exclude(id__in=already_used_ids).first()

                        if not replacement:
                            # All real copies are already inside this deck → cannot replace
                            continue

                        # 5) Replace proxy in each of the deck zones
                        if dv.main_deck.filter(pk=card.pk).exists():
                            dv.main_deck.remove(card)
                            dv.main_deck.add(replacement)

                        if dv.extra_deck.filter(pk=card.pk).exists():
                            dv.extra_deck.remove(card)
                            dv.extra_deck.add(replacement)

                        if dv.side_deck.filter(pk=card.pk).exists():
                            dv.side_deck.remove(card)
                            dv.side_deck.add(replacement)

                        dv.save()
                        self.stdout.write(
                            f"Replaced proxy {card.pk} with real card {replacement.pk} ({replacement.en_name}) "
                            f"in deck version '{dv.name}'."
                        )

                # Optional: remove proxy if unused anywhere
                if not card.deck_versions.exists():
                    self.stdout.write(f"Deleting unused proxy card {card.pk} ({card.en_name}).")
                    card.delete()
                    proxy_removed += 1
            
        self.stdout.write(
            self.style.SUCCESS(
                f"Normalization complete. CardData updated: {updated_carddata}, Cards updated: {updated_cards}, Proxies removed: {proxy_removed}."
            )
        )
