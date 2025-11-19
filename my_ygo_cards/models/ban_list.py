


from django.db import models

class AdvancedBanList(models.Model):
    date = models.DateField(unique=True, help_text="The date when this ban list became effective")


    def __str__(self):
        return f"Advanced Ban List effective from {self.date.strftime('%Y-%m-%d')}"
    
class BanListEntry(models.Model):
    BAN_STATUS_CHOICES = [
        ('Banned', 'Banned'),
        ('Limited', 'Limited'),
        ('Semi-Limited', 'Semi-Limited'),
    ]

    ban_list = models.ForeignKey(
        AdvancedBanList,
        related_name="entries",
        on_delete=models.CASCADE
    )
    card_data = models.ForeignKey(
        'CardData',
        related_name="ban_list_entries",
        on_delete=models.PROTECT
    )
    status = models.CharField(max_length=15, choices=BAN_STATUS_CHOICES)

    def __str__(self):
        return f"{self.card_data}: {self.status} in {self.ban_list}"