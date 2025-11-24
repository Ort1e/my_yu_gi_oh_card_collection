from django.urls import include, path
from django.conf import settings
from django.conf.urls.static import static
from drf_spectacular.views import (
    SpectacularAPIView,
    SpectacularRedocView,
    SpectacularSwaggerView,
)

from .rest_api import deck_builder_api


from .views.deck import AddDeckView, DeckDetailView, DecksView

from .views.cards import CardDetailView, CardsView
from .views.lots.lots import AddLotView, LotDetailView, LotListView
from .views.sellers import SellersView, SellerDetailView

from .views.budget import budget_overview

from .views.index import index



deck_version_patterns = [
    path("", deck_builder_api.DeckVersionDetailAPI.as_view(), name="deckversion-detail"),
    path("clone/", deck_builder_api.DeckVersionCloneAPI.as_view(), name="deckversion-clone"),
    path("proxy/", deck_builder_api.DeckVersionProxyCreateAPI.as_view(), name="deckversion-proxy-create"),
    path("proxy/<int:card_id>/", deck_builder_api.DeckVersionProxyDeleteAPI.as_view(), name="deckversion-proxy-delete"),
    path("card_lists/", deck_builder_api.DeckVersionCardListAPI.as_view(), name="deckversion-cardlists"),
    path("categories/", deck_builder_api.DeckVersionCardCategoryListAPI.as_view(), name="deckversion-categories"),
    path("assign_categories/<int:card_id>/", deck_builder_api.DeckVersionCardCategoryAssignmentAPI.as_view(), name="deckversion-assign-categories"),
    path("categories/delete/<int:category_id>/", deck_builder_api.DeckVersionCardCategoryRemoveAPI.as_view(), name="deckversion-category-delete"),
    path("monte_carlos/", deck_builder_api.DeckVersionMonteCarloAPI.as_view(), name="deckversion-monte-carlo"),
]

urlpatterns = [
    path("", index, name="index"),

    
    path('budget/', budget_overview, name='budget'),

    path("cards/", CardsView.as_view(), name="cards"),
    path("cards/card/<int:pk>/", CardDetailView.as_view(), name="card"),

    path("sellers/", SellersView.as_view(), name="sellers"),
    path("sellers/seller/<int:pk>", SellerDetailView.as_view(), name="seller"),

    path("lots/", LotListView.as_view(), name="lots"),
    path("lots/<int:pk>/", LotDetailView.as_view(), name="lot"),
    path("lots/add/", AddLotView.as_view(), name="add_lot"),
    path("accounts/", include("django.contrib.auth.urls")),

    path("decks/", DecksView.as_view(), name="decks"),
    path("decks/add/", AddDeckView.as_view(), name="add_deck"),
    path("decks/deck/<int:pk>", DeckDetailView.as_view(), name="deck"),
    path("deck_builder/<int:deck_version_id>/", deck_builder_api.DeckBuilderView.as_view(), name="deck_builder"),

    # --------------------- API Paths --------------------- #
    path("api/deck_versions/<int:deck_version_id>/", include(deck_version_patterns)),

    path("api/decks/<int:deck_id>/import_ydke/", deck_builder_api.DeckImportYdkeAPI.as_view(), name="deck-import-ydke"),

    path("api/schema/", SpectacularAPIView.as_view(), name="schema")
    
] + static(settings.MEDIA_URL, document_root=settings.MEDIA_ROOT)