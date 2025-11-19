from django.contrib import admin


from .models import Card, CardData, Deck, DeckVersion, Tournament, Seller, \
    Lot, SellerSource, Unite, MonthlyBudget, Apport, CardCategory, CardCategoryAssignment, CardConditionalCategory, \
    AdvancedBanList, BanListEntry




# Register your models here.
admin.site.register(Card)
admin.site.register(Seller)
admin.site.register(SellerSource)
admin.site.register(Lot)
admin.site.register(Unite)
admin.site.register(MonthlyBudget)
admin.site.register(CardData)
admin.site.register(DeckVersion)
admin.site.register(Apport)
admin.site.register(Tournament)
admin.site.register(Deck)
admin.site.register(CardCategory)
admin.site.register(CardCategoryAssignment)
admin.site.register(CardConditionalCategory)
admin.site.register(AdvancedBanList)
admin.site.register(BanListEntry)