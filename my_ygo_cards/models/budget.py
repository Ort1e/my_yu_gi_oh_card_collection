
from django.db import models

class MonthlyBudget(models.Model):
    month = models.DateField(unique=True, help_text="Use the first day of the month")
    amount = models.DecimalField(max_digits=10, decimal_places=2)

    def __str__(self):
        return f"Budget for {self.month.strftime('%B %Y')}: {self.amount}€"

    @property
    def total_with_apports(self):
        """Return the base amount + all ponctual apports for this month."""
        apports_sum = self.apports.aggregate(total=models.Sum("amount"))["total"] or 0 # type: ignore
        return self.amount + apports_sum


class Apport(models.Model):
    budget = models.ForeignKey(
        MonthlyBudget,
        related_name="apports",
        on_delete=models.CASCADE
    )
    description = models.CharField(max_length=255, blank=True)
    amount = models.DecimalField(max_digits=10, decimal_places=2)
    date = models.DateField()

    def __str__(self):
        return f"{self.amount}€ on {self.date} ({self.description})"
