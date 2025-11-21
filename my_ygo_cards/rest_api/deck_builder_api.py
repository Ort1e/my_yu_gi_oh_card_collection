from typing import TypedDict, List, cast

from django import forms
from django.urls import reverse
from django.views import View
from django.contrib.auth.mixins import LoginRequiredMixin
from django.shortcuts import redirect, render, get_object_or_404

from rest_framework import serializers

from drf_spectacular.utils import extend_schema

from my_ygo_cards.monte_carlos.monte_carlos import run_monte_carlo_simulation
from my_ygo_cards.rest_api.serializers import CardCategoryAssignmentSerializer, CardCategorySerializer, CardSerializer, DeckVersionSerializer, DeckVersionUpdateSerializer, DeckYdkeImportSerializer
from my_ygo_cards.views.cards import CardFilterSerializer, filter_cards_queryset
from rest_framework.views import APIView
from rest_framework.response import Response
from rest_framework import status

from my_ygo_cards.models import Card, CardData, Deck, DeckVersion, Tournament, CardCategory, CardCategoryAssignment, AdvancedBanList


class TournamentForm(forms.ModelForm):
    class Meta:
        model = Tournament
        fields = ['name', 'date', 'location', 'notes']
        widgets = {
            'date': forms.DateInput(attrs={'type': 'date'}),
        }



class DeckBuilderView(LoginRequiredMixin, View):
    template_name = "my_ygo_cards/deck_builder.html"


    def get(self, request, deck_version_id, *args, **kwargs):
        deck = get_object_or_404(DeckVersion, id=deck_version_id)
        all_version = DeckVersion.objects.filter(deck=deck.deck).order_by("-version_name")
        tournaments_form : TournamentForm 
        if deck.tournament:
            tournaments_form = TournamentForm(instance=deck.tournament)
        else:
            tournaments_form = TournamentForm()

        return render(request, self.template_name, {
            "deck": deck,
            "all_versions": all_version,
            "tournaments_form": tournaments_form,
            'all_card_types': CardData.get_all_card_types(),
            "ban_lists": AdvancedBanList.objects.all().order_by("-date"),

            **deck.get_prices()
        })
    
    def post(self, request, deck_version_id, *args, **kwargs):
        deck_version = get_object_or_404(DeckVersion, id=deck_version_id)

        # test the presence of the form data
        tournament_form = TournamentForm(request.POST)
        if tournament_form.is_bound and tournament_form.is_valid():
            tournament = tournament_form.save()
            deck_version.tournament = tournament
            deck_version.save()
            return redirect("deck_builder", deck_version_id=deck_version.pk)
        

# -------------------- Deck Versions api ----------------------- #

class DeckVersionUpdateData(TypedDict, total=False):
    version_name: str
    main_deck: List[int]
    extra_deck: List[int]
    side_deck: List[int]
    ban_list_id: int


class DeckVersionDetailAPI(APIView):
    """GET, PATCH, DELETE deck version"""

    @extend_schema(
        responses=DeckVersionSerializer,
    )
    def get(self, request, deck_version_id):
        deck = get_object_or_404(DeckVersion, pk=deck_version_id)
        return Response(DeckVersionSerializer(deck).data)

    @extend_schema(
        request=DeckVersionUpdateSerializer,
        responses=DeckVersionSerializer,
    )
    def patch(self, request, deck_version_id):
        deck = get_object_or_404(DeckVersion, pk=deck_version_id)
        serializer = DeckVersionUpdateSerializer(deck, data=request.data, partial=True)

        if serializer.is_valid():
            data = cast(DeckVersionUpdateData, serializer.validated_data) # warn : maybe a bit unsafe

            if 'version_name' in data:
                deck.version_name = data['version_name']

            if 'main_deck' in data:
                deck.main_deck.set(Card.objects.filter(id__in=data['main_deck']))

            if 'extra_deck' in data:
                deck.extra_deck.set(Card.objects.filter(id__in=data['extra_deck']))

            if 'side_deck' in data:
                deck.side_deck.set(Card.objects.filter(id__in=data['side_deck']))
            
            if 'ban_list_id' in data:
                if data['ban_list_id'] is None:
                    deck.ban_list = None
                else:
                    deck.ban_list = get_object_or_404(AdvancedBanList, pk=data['ban_list_id']) #type: ignore

            deck.save()
            return Response(DeckVersionSerializer(deck).data)

        return Response(serializer.errors, status=status.HTTP_400_BAD_REQUEST)

    @extend_schema(
        responses={204: None}
    )
    def delete(self, request, deck_version_id):
        deck = get_object_or_404(DeckVersion, pk=deck_version_id)
        deck.delete()
        return Response(status=status.HTTP_204_NO_CONTENT)
    

class DeckVersionCloneInputSerializer(serializers.Serializer):
    name = serializers.CharField(
        max_length=255,
        help_text="Name of the new cloned deck version"
    )

class DeckVersionCloneAPI(APIView):
    """POST /api/deck_versions/{deck_version_id}/clone/"""

    @extend_schema(
        request=DeckVersionCloneInputSerializer,
        responses={
            201: {
                "type": "object",
                "properties": {
                    "deck_id": {"type": "integer"},
                    "deck_url": {"type": "string"}
                }
            }
        }
    )
    def post(self, request, deck_version_id):
        old = get_object_or_404(DeckVersion, pk=deck_version_id)
        serializer = DeckVersionCloneInputSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        name = serializer.data.get("name")

        new = DeckVersion.objects.create(deck=old.deck, version_name=name, tournament=old.tournament)

        def copy_zone(from_zone, to_zone):
            for card in getattr(old, from_zone).all():
                if card.is_proxy:
                    card.pk = None
                    card._state.adding = True
                    card.save()
                getattr(new, to_zone).add(card)

        copy_zone("main_deck", "main_deck")
        copy_zone("extra_deck", "extra_deck")
        copy_zone("side_deck", "side_deck")

        new_url = reverse("deck_builder", kwargs={"deck_version_id": new.pk})

        return Response({
            "deck_id": new.pk,
            "deck_url": new_url
        }, status=status.HTTP_201_CREATED)

class DeckVersionProxyCreateInputSerializer(serializers.Serializer):
    name = serializers.CharField(
        max_length=255,
        help_text="Name of the proxy card"
    )
    zone = serializers.ChoiceField(
        choices=["main", "extra", "side"],
        default="main",
        help_text="Deck zone to add the proxy card"
    )

class DeckVersionProxyCreateAPI(APIView):
    """POST /api/deck_versions/{deck_version_id}/proxy/"""

    @extend_schema(
        request=DeckVersionProxyCreateInputSerializer,
        responses=CardSerializer,
    )
    def post(self, request, deck_version_id):
        deck = get_object_or_404(DeckVersion, pk=deck_version_id)
        serializer = DeckVersionProxyCreateInputSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)

        card = deck.add_proxy_card(name=serializer.data.get("name"), zone=serializer.data.get("zone", "main"))
        return Response(CardSerializer(card).data, status=status.HTTP_201_CREATED)

class DeckVersionProxyDeleteAPI(APIView):
    """DELETE /api/deck_versions/{deck_version_id}/proxy/{card_id}/"""

    @extend_schema(
        responses={204: None}
    )
    def delete(self, request, deck_version_id, card_id):
        deck = get_object_or_404(DeckVersion, pk=deck_version_id)
        card = get_object_or_404(Card, pk=card_id, is_proxy=True)
        deck.remove_card(card)
        return Response(status=status.HTTP_204_NO_CONTENT)
    

class DeckVersionCardListAPI(APIView):
    """
    POST /api/deck_versions/{deck_version_id}/card_lists/
    Filters cards using filter_cards_queryset.
    Optional query params:
      - deck_id: exclude cards already in this deck
      - name, card_type, proxy, sold, code, status
      - limit: limit number of results
    """

    @extend_schema(
        request=CardFilterSerializer,
        responses=CardSerializer(many=True),
    )
    def post(self, request, deck_version_id):
        deck_version = get_object_or_404(DeckVersion, pk=deck_version_id)

        params = dict(request.query_params)
        # Always filter out sold/proxy if needed (optional)
        params["proxy"] = "false"
        params["sold"] = "false"

        filters = CardFilterSerializer(data=params)
        filters.is_valid(raise_exception=True)

        

        qs = filter_cards_queryset(Card.objects.all(), filters)

        # Exclude cards already in deck
        
        excluded_ids = list(deck_version.main_deck.values_list("id", flat=True)) + \
                        list(deck_version.extra_deck.values_list("id", flat=True)) + \
                        list(deck_version.side_deck.values_list("id", flat=True))
        qs = qs.exclude(id__in=excluded_ids)

        # Apply limit
        if "limit" in params:
            try:
                qs = qs[:int(params["limit"])]
            except ValueError:
                return Response({"error": "Invalid limit"}, status=status.HTTP_400_BAD_REQUEST)

        serializer = CardSerializer(qs, many=True)
        return Response(serializer.data, status=status.HTTP_200_OK)

# .......... categories

class DeckVersionCardCategoryListAPI(APIView):
    """
    GET  -> List all categories for a deck
    POST -> Create a new category
    """

    @extend_schema(
        responses=CardCategorySerializer(many=True),
    )
    def get(self, request, deck_version_id):
        categories = CardCategory.objects.filter(deck_version_id=deck_version_id)
        serializer = CardCategorySerializer(categories, many=True)
        return Response(serializer.data)

    @extend_schema(
        request={
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the new category"
                }
            }
        },
        responses=CardCategorySerializer,
    )
    def post(self, request, deck_version_id):
        
        name = request.data.get("name")
        cat = CardCategory.objects.create(name=name, deck_version_id=deck_version_id)
        return Response(CardCategorySerializer(cat).data, status=status.HTTP_201_CREATED)


class DeckVersionCardCategoryRemoveAPI(APIView):
    """
    DELETE -> remove category + its assignments
    """

    @extend_schema(
        responses={204: None}
    )
    def delete(self, request, deck_version_id, category_id):
        # Delete assignments first
        CardCategoryAssignment.objects.filter(category_id=category_id).delete()
        # Delete the category
        CardCategory.objects.filter(id=category_id, deck_version_id=deck_version_id).delete()
        response_data = {"status": "deleted"}
        return Response(response_data, status=status.HTTP_204_NO_CONTENT)


class DeckVersionCardCategoryAssignmentAPI(APIView):
    """
    GET  -> return all categories for a card in a deck, marking assigned ones
    POST -> assign/unassign a card to a category
    """

    @extend_schema(
        responses={
            200: {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "integer"},
                        "name": {"type": "string"},
                        "assigned": {"type": "boolean"}
                    }
                }
            }
        }
    )
    def get(self, request, card_id, deck_version_id):
        categories = CardCategory.objects.filter(deck_version_id=deck_version_id)
        assigned = set(
            CardCategoryAssignment.objects.filter(card_id=card_id).values_list("category_id", flat=True)
        )

        data = [
            {"id": cat.pk, "name": cat.name, "assigned": cat.pk in assigned}
            for cat in categories
        ]
        return Response(data)

    @extend_schema(
        request=CardCategoryAssignmentSerializer,
        responses={200: {"type": "object", "properties": {"status": {"type": "string"}}}}
    )
    def post(self, request, card_id, deck_version_id):
        """
        JSON payload: { category_id, assigned: true/false }
        """

        serializer = CardCategoryAssignmentSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = cast(dict, serializer.validated_data)

        category_id = data["category_id"]
        get_object_or_404(CardCategory, pk=category_id, deck_version_id=deck_version_id)

        assigned = data["assigned"]

        if assigned:
            CardCategoryAssignment.objects.get_or_create(card_id=card_id, category_id=category_id)
        else:
            CardCategoryAssignment.objects.filter(card_id=card_id, category_id=category_id).delete()

        return Response({"status": "ok"})


# -------------------- Decks api ----------------------- #

class DeckImportYdkeAPI(APIView):
    """
    POST /api/decks/{deck_id}/import_ydke/
    Creates a new DeckVersion from a YDKE URL.
    """
    @extend_schema(
        request=DeckYdkeImportSerializer,
        responses={
            201: {
                "type": "object",
                "properties": {
                    "status": {"type": "string"},
                    "deck_id": {"type": "integer"},
                    "deck_url": {"type": "string"}
                }
            }
        }
    )
    def post(self, request, deck_id):
        deck = get_object_or_404(Deck, pk=deck_id)
        serializer = DeckYdkeImportSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = cast(dict, serializer.validated_data)

        ydke_url = data["ydke_url"]
        deck_name = data["name"]

        try:
            # Assuming you want parent_deck_id to be None here, or you can modify as needed
            deck_version = DeckVersion.from_ydke(deck_id, ydke_url=ydke_url, name=deck_name)
            deck_url = reverse("deck_builder", kwargs={"deck_id": deck_version.pk})

            return Response({
                "status": "ok",
                "deck_id": deck_version.pk,
                "deck_url": deck_url
            }, status=status.HTTP_201_CREATED)

        except ValueError as e:
            return Response({"error": str(e)}, status=status.HTTP_400_BAD_REQUEST)
        
# --------------------- Monte carlos api ---------------------- #

class DeckVersionMonteCarloInputSerializer(serializers.Serializer):
    num_simulations = serializers.IntegerField(
        default=1000,
        min_value=1,
        help_text="Number of Monte Carlo simulations to run"
    )
    num_cards = serializers.IntegerField(
        default=5,
        min_value=1,
        help_text="Number of cards to draw per simulation"
    )

class DeckVersionMonteCarloAPI(APIView):
    """
    POST /api/deck_versions/{deck_version_id}/monte_carlos/
    Runs Monte Carlo simulations on the deck version.
    """

    @extend_schema(
        request=DeckVersionMonteCarloInputSerializer,
        responses={
            200: {
                "type": "object",
                "properties": {
                    "status": {"type": "string"},
                    "results": {"type": "object"}
                }
            }
        }
    )
    def post(self, request, deck_version_id):
        deck_version = get_object_or_404(DeckVersion, pk=deck_version_id)
        serializer = DeckVersionMonteCarloInputSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)


        num_simulations = serializer.data.get("num_simulations", 1000)
        num_cards = serializer.data.get("num_cards", 5)

        try:
            num_simulations = int(num_simulations)
            num_cards = int(num_cards)
            if num_simulations <= 0:
                raise ValueError("Number of simulations must be positive.")
            
            if num_cards <= 0:
                raise ValueError("Number of cards must be positive.")
        except (ValueError, TypeError):
            return Response({"error": "Invalid number of simulations."}, status=status.HTTP_400_BAD_REQUEST)

        results = run_monte_carlo_simulation(deck_version, num_cards, num_simulations)

        return Response({
            "status": "ok",
            "results": results
        }, status=status.HTTP_200_OK)