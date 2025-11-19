import re


from django.db import models
from django.dispatch import receiver
from django.db.models.signals import pre_save, post_delete

from my_ygo_cards.models.card import Card, CardData
from my_ygo_cards.models.deck import DeckVersion


def strip_parentheses(name: str) -> str:
    """Remove parentheses and their content from a string."""
    return re.sub(r"\s*\([^)]*\)", "", name).strip()

@receiver(pre_save, sender=CardData)
def normalize_carddata_name(sender, instance, **kwargs):
    if instance.en_name:
        instance.en_name = strip_parentheses(instance.en_name).lower()

@receiver(pre_save, sender=Card)
def normalize_card_name(sender, instance, **kwargs):
    if instance.en_name:
        instance.en_name = strip_parentheses(instance.en_name).lower()


@receiver(post_delete, sender=DeckVersion)
def cleanup_proxy_cards(sender, instance, **kwargs):
    """
    After a deck is deleted, remove all proxy cards that
    are no longer linked to any deck.
    """
    orphan_proxies = Card.objects.filter(is_proxy=True).filter(
        ~models.Q(main_decks__isnull=False) &
        ~models.Q(extra_decks__isnull=False) &
        ~models.Q(side_decks__isnull=False)
    ).distinct()
    for card in orphan_proxies:
        print(f"Deleting orphan proxy card: {card.name} (ID: {card.pk})")
        card.delete()
