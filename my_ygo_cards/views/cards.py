


from typing import Any, Mapping
from urllib.parse import urlencode
from django.views import generic
from django.db.models import Exists, OuterRef, QuerySet

from ..models import Card, CardData, Lot, Unite

from django.db.models import Exists, OuterRef

def filter_cards_queryset(queryset: QuerySet[Card], params: Mapping[str, Any]) -> QuerySet[Card]:
    status = params.get("status")
    proxy = params.get("proxy")
    name = params.get("name")
    card_type = params.get("card_type")
    code = params.get("code")
    sold = params.get("sold")


    if status:
        queryset = queryset.filter(last_known_status__iexact=status)

    if proxy == "true":
        queryset = queryset.filter(is_proxy=True)
    elif proxy == "false":
        queryset = queryset.filter(is_proxy=False)

    if name:
        queryset = queryset.filter(name__icontains=name) | queryset.filter(en_name__icontains=name)
    
    if code:
        queryset = queryset.filter(code__icontains=code)
    
    if card_type:
        queryset = queryset.filter(card_data__card_type__iexact=card_type)

    if sold in ["true", "false"]:
        # Create a subquery to check if there is a sold Unite
        sold_subquery = Unite.objects.filter(
            card=OuterRef('pk'),
            lot__lot_type=Lot.SALE,
            lot__is_cancelled=False
        )
        if sold == "true":
            queryset = queryset.annotate(is_sold_flag=Exists(sold_subquery)).filter(is_sold_flag=True)
        else:
            queryset = queryset.annotate(is_sold_flag=Exists(sold_subquery)).filter(is_sold_flag=False)


    return queryset.distinct().order_by('en_name', 'id')



class CardsView(generic.ListView):
    paginate_by = 10
    template_name = 'my_ygo_cards/list_base.html'
    context_object_name = 'object_list'  # important: expected by list_base.html

    def get_queryset(self):
        queryset = Card.objects.all()
        return filter_cards_queryset(queryset, self.request.GET)
    

    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        # Build querystring excluding 'page' parameter (keep it in the links)
        get_params = self.request.GET.copy()
        get_params.pop('page', None)
        context['querystring'] = urlencode(get_params)
        context.update({
            'page_title': 'All Cards',
            'item_template': 'my_ygo_cards/partials/one_card_list.html',
            'include_filter': 'my_ygo_cards/partials/card_filter.html',
            'object_name': 'card',
            'list_length': len(Card.objects.all()),
            'all_card_types': CardData.get_all_card_types(),
        })
        return context
    

class CardDetailView(generic.DetailView):
    model = Card
    template_name = 'my_ygo_cards/card_detail.html'

    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        card = self.get_object()

        unite = card.unite_set.first() # type: ignore
        if unite:
            context['lot'] = unite.lot
            context['price'] = unite.price
        else:
            context['lot'] = None
            context['price'] = None
        
        return context