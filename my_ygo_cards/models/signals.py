import re


from django.db import models
from django.dispatch import receiver
from django.db.models.signals import post_delete

from my_ygo_cards.models.card import Card
from my_ygo_cards.models.deck import DeckVersion

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
