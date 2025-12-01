import re
from django.db import models
from django.db.models import Q

from my_ygo_cards.image_downloader.image_downloader import ImageDownloader

def normalize_card_name(name: str) -> str:
    """
    Normalize the card name by stripping whitespace and converting to lowercase.
    Additional normalization logic can be added here.
    """
    return re.sub(r"\s*\([^)]*\)", "", name).strip().lower()


class CardData(models.Model):
    """
    Reference data for a card (from external API).
    Unique per card (identified by English name).
    """
    en_name = models.CharField(max_length=255, unique=True)
    ygopro_id = models.IntegerField(null=True, blank=True, unique=True)
    card_type = models.CharField(max_length=100, null=True, blank=True)
    json_data = models.JSONField()
   

    @property
    def image_url(self) -> str:
        """
        Return the URL of the card image.
        """
        if not self.en_name:
            return ""
        downloader = ImageDownloader(self.en_name)
        if not downloader.image_url:
            return ""
        else:
            return downloader.image_url

    def __str__(self):
        return self.en_name
    
    @staticmethod
    def get_all_card_types():
        return CardData.objects.values_list('card_type', flat=True).distinct().order_by('card_type')
    
    @classmethod
    def get_or_fetch(cls, en_name):
        """
        Get the CardData by en_name, or fetch from external API if missing.
        """
        cleaned_name = ImageDownloader.clean_card_name(en_name)
        try:
            return cls.objects.get(en_name=cleaned_name)
        except cls.DoesNotExist:
            # Call your API here
            api_data = ImageDownloader.get_card_data(cleaned_name)  # You need to implement this
            if not api_data:
                return None  # API returned nothing
            card_data = cls(
                en_name=cleaned_name,
                json_data=api_data,
                ygopro_id=api_data.get("id"),
                card_type=api_data.get("type"),
            )
            card_data.save()
            return card_data

class Card(models.Model):
    STATUS_CHOICES = [
        ('MT', 'Mint'),
        ('NM', 'Near Mint'),
        ('EX', 'Excellent'),
        ('G', 'Good'),
        ('P', 'Poor'),
    ]

    name = models.CharField(max_length=255)
    en_name = models.CharField(max_length=255, null=True, blank=True)
    card_data = models.ForeignKey(CardData, null=True, blank=True, on_delete=models.SET_NULL, related_name="copies")
    code = models.CharField(max_length=20, null=True, blank=True)
    last_known_status = models.CharField(max_length=2, null=True, blank=True, choices=STATUS_CHOICES)
    is_proxy = models.BooleanField(default=False)

    def __str__(self):
        return f"{self.name} ({self.en_name})" if self.en_name and self.name != self.en_name else self.name
    
    @property
    def data(self):
        """
        Returns the CardData. If missing, fetch from API automatically.
        """
        if self.card_data is None and self.en_name:
            self.card_data = CardData.get_or_fetch(self.en_name)
            if self.card_data:
                self.save(update_fields=['card_data'])
        return self.card_data
    
    @property
    def deck_versions(self):
        """Return all DeckVersions (main/extra/side) containing this card."""
        from my_ygo_cards.models.deck import DeckVersion
        return DeckVersion.objects.filter(
            Q(main_deck=self) | Q(extra_deck=self) | Q(side_deck=self)
        ).distinct()
    
    @property
    def is_sold(self):
        from my_ygo_cards.models.lot import Lot, Unite
        """Return True if this card is in a sold lot."""
        return Unite.objects.filter(card=self, lot__lot_type=Lot.SALE, lot__is_cancelled=False).exists()


    def save(self, *args, **kwargs):
            if self.en_name:
                self.en_name = normalize_card_name(self.en_name)
            super().save(*args, **kwargs)