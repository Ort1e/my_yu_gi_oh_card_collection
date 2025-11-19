from datetime import timedelta
from decimal import Decimal
from django.db import models


SHPMENT_FOLDER = 'shipment_files/'

class Seller(models.Model):
    name = models.CharField(max_length=255, unique=True)
    is_person = models.BooleanField(default=True)
    source = models.ForeignKey('SellerSource', on_delete=models.SET_NULL, null=True, blank=True)

    class Meta:
        unique_together = (('name', 'source'),)

    def __str__(self):
        return self.name

class SellerSource(models.Model):
    name = models.CharField(max_length=255, unique=True)
    url = models.URLField(max_length=200, blank=True, null=True)

    def __str__(self):
        return self.name


class Lot(models.Model):
    PURCHASE = "purchase"
    SALE = "sale"
    LOT_TYPE_CHOICES = [
        (PURCHASE, "Purchase"),
        (SALE, "Sale"),
    ]
    lot_type = models.CharField(
        max_length=10,
        choices=LOT_TYPE_CHOICES,
        default=PURCHASE,
    )

    price = models.DecimalField(max_digits=10, decimal_places=2)
    buy_date = models.DateField()
    received_date = models.DateField(null=True, blank=True)
    seller = models.ForeignKey(Seller, on_delete=models.SET_NULL, default=None, null=True, blank=True)
    cards = models.ManyToManyField("Card", through='Unite')
    is_cancelled = models.BooleanField(default=False)
    no_card_price = models.DecimalField(max_digits=10, decimal_places=2, default=Decimal('0.00'))
    shipment_file = models.FileField(upload_to=SHPMENT_FOLDER, null=True, blank=True)

    @property
    def shipping_cost(self) -> Decimal:        
        all_unites = Unite.objects.filter(lot=self)
        total_card_price = sum((unite.price for unite in all_unites if unite.price is not None), Decimal('0.00'))

        return self.price - (self.no_card_price + total_card_price)

    def receiving_time(self) -> timedelta | None:
        if self.received_date:
            return (self.received_date - self.buy_date)
        return None

    def __str__(self):
        return f"Lot {self.buy_date} - {self.seller.name if self.seller else 'No Seller'}"


class Unite(models.Model):
    price = models.DecimalField(max_digits=10, decimal_places=2, null=True, blank=True)
    lot = models.ForeignKey(Lot, on_delete=models.CASCADE)
    card = models.ForeignKey("Card", on_delete=models.CASCADE)

    def __str__(self):
        return f"Unite {self.card.name} - Price: {self.price if self.price else 'N/A'}"

    class Meta:
            constraints = [
                models.UniqueConstraint(
                    fields=["lot", "card"], name="lot_card_unite_unique"
                )
            ]