
from django.db import models

class CardCategory(models.Model):
    name = models.CharField(max_length=255)
    deck_version = models.ForeignKey("DeckVersion", on_delete=models.CASCADE, related_name="categories")

    def __str__(self):
        return f"{self.name} ({self.deck_version})"
    
    def has_card(self, card):
        return CardCategoryAssignment.objects.filter(category=self, card=card).exists()


class CardCategoryAssignment(models.Model):
    category = models.ForeignKey(CardCategory, on_delete=models.CASCADE)
    card = models.ForeignKey("Card", on_delete=models.CASCADE)

    def __str__(self):
        return f"{self.card} > {self.category}"

    class Meta:
        unique_together = (("category", "card"),)


class CardConditionalCategory(CardCategory):
    condition_description = models.TextField()
    categories_or_conditions = models.ManyToManyField(CardCategory, blank=True, related_name="conditional_categories_or_conditions")
    categorie_true = models.ForeignKey(CardCategory, on_delete=models.CASCADE, related_name="conditional_true")
    categorie_false = models.ForeignKey(CardCategory, on_delete=models.CASCADE, related_name="conditional_false", null=True, blank=True)

    def __str__(self):
        return f"IF {self.condition_description} THEN {self.categorie_true} ELSE {self.categorie_false if self.categorie_false else 'None'}"
    
    def evaluate_for_categories(self, categories: list[CardCategory]) -> CardCategory | None:
        """
            Evaluate the conditional category based on the provided categories.
            Returns the true category if any of the provided categories match the condition,
            otherwise returns the false category (if defined).
            
            NOTE: Do the following if you don't want to query the database multiple times:
            conditional_categories = CardConditionalCategory.objects.prefetch_related(
                Prefetch('categories_or_conditions')
            )
        """
        related_ids = {cat.pk for cat in self.categories_or_conditions.all()}

        for cat in categories:
            if cat.pk in related_ids:
                return self.categorie_true

        return self.categorie_false
