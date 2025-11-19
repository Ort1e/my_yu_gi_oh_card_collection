import base64
import struct
from django.db import models
from django.db.models import Min

from my_ygo_cards.image_downloader.image_downloader import ImageDownloader



class Tournament(models.Model):
    name = models.CharField(max_length=255)
    date = models.DateField()
    location = models.CharField(max_length=255, blank=True, null=True)
    notes = models.TextField(blank=True, null=True)

    def __str__(self):
        return f"{self.name} ({self.date})"

class Deck(models.Model):
    name = models.CharField(max_length=100)
    description = models.TextField(blank=True, null=True)

    def __str__(self):
        return self.name


class DeckVersion(models.Model):
    deck = models.ForeignKey(Deck, on_delete=models.CASCADE, related_name="versions", null=True, blank=True)
    version_name = models.CharField(max_length=100)

    @property
    def name(self):
        deck_name = self.deck.name if self.deck else "No Deck"
        return f"{deck_name} - {self.version_name}"

    main_deck = models.ManyToManyField(
        "Card", related_name="main_decks", blank=True
    )
    extra_deck = models.ManyToManyField(
        "Card", related_name="extra_decks", blank=True
    )
    side_deck = models.ManyToManyField(
        "Card", related_name="side_decks", blank=True
    )

    tournament = models.ForeignKey(
        Tournament, on_delete=models.SET_NULL, null=True, blank=True
    )

    ban_list = models.ForeignKey(
        "AdvancedBanList",
        on_delete=models.SET_NULL,
        null=True,
        blank=True,
    )


    def __str__(self):
        return self.name
    

    def _ids_to_base64(self, ids):
        """Convert list of int IDs → base64 string (like YDKE expects)."""
        byte_array = b"".join(struct.pack("<I", id) for id in ids)
        return base64.b64encode(byte_array).decode("ascii")

    def _get_ids(self, cards, include_proxies=True, only_proxies=False):
        """
        Extract ygopro_id from CardData with filtering for proxies.
        """
        ids = []
        for card in cards:
            if not card.data or not card.data.ygopro_id:
                continue
            if only_proxies and not card.is_proxy:
                continue
            if not include_proxies and card.is_proxy:
                continue
            ids.append(card.data.ygopro_id)
        return ids

    def get_ydke_url(self, include_proxies=True, only_proxies=False):
        """
        Build a YDKE URL for this deck.

        Args:
            include_proxies (bool): If False, skip proxy cards.
            only_proxies (bool): If True, include only proxy cards.

        Returns:
            str: YDKE deck URL
        """
        main_ids = self._get_ids(self.main_deck.all(), include_proxies, only_proxies)
        extra_ids = self._get_ids(self.extra_deck.all(), include_proxies, only_proxies)
        side_ids = self._get_ids(self.side_deck.all(), include_proxies, only_proxies)

        main_b64 = self._ids_to_base64(main_ids)
        extra_b64 = self._ids_to_base64(extra_ids)
        side_b64 = self._ids_to_base64(side_ids)

        return f"ydke://{main_b64}!{extra_b64}!{side_b64}!"
    
    @property
    def ydke_with_proxies(self):
        return self.get_ydke_url()

    @property
    def ydke_without_proxies(self):
        return self.get_ydke_url(include_proxies=False)

    @property
    def ydke_only_proxies(self):
        return self.get_ydke_url(only_proxies=True)
    
    @staticmethod
    def decode_ydke_part(b64string: str) -> list[int]:
        """
        Decode a base64 string into a list of ygopro_ids (ints).
        Each ID is a 4-byte little-endian integer.
        """
        if not b64string:
            return []
        raw = base64.b64decode(b64string)
        return [struct.unpack("<I", raw[i:i+4])[0] for i in range(0, len(raw), 4)]

    @classmethod
    def parse_ydke(cls, ydke_url: str) -> tuple[list[int], list[int], list[int]]:
        """
        Parse a YDKE URL and return (main_ids, extra_ids, side_ids).
        """
        if not ydke_url.startswith("ydke://"):
            raise ValueError("Invalid YDKE URL")

        content = ydke_url[len("ydke://") :]
        if content.endswith("!"): # remove the last one
            content = content[:-1]

        parts = content.split("!")

        if len(parts) < 3:
            raise ValueError("YDKE must contain main, extra, and side parts")

        main_ids = cls.decode_ydke_part(parts[0])
        extra_ids = cls.decode_ydke_part(parts[1])
        side_ids = cls.decode_ydke_part(parts[2])

        return main_ids, extra_ids, side_ids

    @classmethod
    def from_ydke(cls, deck_id : int | None, ydke_url: str, name="Imported Deck"):
        """
        Import a deck from a YDKE URL.
        """
        from my_ygo_cards.models.card import Card, CardData

        main_ids, extra_ids, side_ids = cls.parse_ydke(ydke_url)

        version_deck = cls.objects.create(version_name=name)
        if deck_id:
            deck = Deck.objects.get(id=deck_id)
            version_deck.deck = deck

        def add_cards(ids, zone):
            used_card_ids = set()

            for ygopro_id in ids:
                try:
                    card_data = CardData.objects.get(ygopro_id=ygopro_id)
                except CardData.DoesNotExist:
                    api_data = ImageDownloader.get_card_data_by_id(ygopro_id)
                    if api_data:
                        card_data = CardData.get_or_fetch(api_data["name"])
                    else:
                        card_data = None

                    

                if card_data:
                    en_name = card_data.en_name
                    available_cards = (
                        Card.objects.filter(en_name=en_name, is_proxy=False)
                        .exclude(id__in=used_card_ids)
                    )
                    if available_cards.exists():
                        card = available_cards.first()
                        if card:
                            used_card_ids.add(card.pk)
                    else:
                        card = Card.objects.create(
                            name=en_name,
                            en_name=en_name,
                            card_data=card_data,
                            is_proxy=True,
                        )
                else:
                    en_name = f"Unknown-{ygopro_id}"
                    card = Card.objects.create(
                        name=en_name,
                        en_name=en_name,
                        card_data=None,
                        is_proxy=True,
                    )

                if zone == "main":
                    version_deck.main_deck.add(card)
                elif zone == "extra":
                    version_deck.extra_deck.add(card)
                elif zone == "side":
                    version_deck.side_deck.add(card)

        add_cards(main_ids, "main")
        add_cards(extra_ids, "extra")
        add_cards(side_ids, "side")

        version_deck.save()
        return version_deck
    
    def add_proxy_card(self, name, zone="main"):
        """
        Add a proxy card to this deck.
        zone: 'main', 'extra', or 'side'
        """
        from my_ygo_cards.models.card import Card, CardData
        card = Card.objects.create(name=name, en_name=name, is_proxy=True)
        card_data = CardData.get_or_fetch(name)
        if card_data:
            card.card_data = card_data
            card.save(update_fields=['card_data'])
        if zone == "main":
            self.main_deck.add(card)
        elif zone == "extra":
            self.extra_deck.add(card)
        elif zone == "side":
            self.side_deck.add(card)
        self.save()
        return card
    
    def remove_card(self, card):
        """
        Remove a card from any deck zone.
        Deletes proxy cards if they are no longer in any deck.
        """
        self.main_deck.remove(card)
        self.extra_deck.remove(card)
        self.side_deck.remove(card)
        self.save()
        
        if card.is_proxy and not (card.main_decks.exists() or card.extra_decks.exists() or card.side_decks.exists()):
            card.delete()

    def _calculate_deck_price(self, cards_queryset):
        """Helper to calculate total price for a given deck part."""
        from my_ygo_cards.models.lot import Unite
        total = 0
        for card in cards_queryset:
            # get all unites (could be multiple lots, choose cheapest or avg)
            unites = Unite.objects.filter(card=card, price__isnull=False)
            if unites.exists():
                # You can switch to min() if you prefer cheapest instead of average
                avg_price = unites.aggregate(avg=Min("price"))["avg"]
                total += avg_price if avg_price else 0
        return round(total, 2)

    def get_prices(self):
        """Return dict with total price for each deck part."""
        main_price = self._calculate_deck_price(self.main_deck.all())
        extra_price = self._calculate_deck_price(self.extra_deck.all())
        side_price = self._calculate_deck_price(self.side_deck.all())

        total_price = round(main_price + extra_price + side_price, 2)

        return {
            "main_deck_price": main_price,
            "extra_deck_price": extra_price,
            "side_deck_price":  side_price,
            "total_deck_price": total_price,
        }
