

from django.views import generic

from ..models import Seller

class SellersView(generic.ListView):
    paginate_by = 10
    template_name = 'my_ygo_cards/list_base.html'
    context_object_name = 'object_list'  # important: expected by list_base.html

    def get_queryset(self):
        queryset = Seller.objects.all()

        return queryset.distinct().order_by('name', 'id')
    
    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        context.update({
            'page_title': 'All Sellers',
            'item_template': 'my_ygo_cards/partials/one_seller_list.html',
            'object_name': 'seller',
            'list_length': len(Seller.objects.all()),
        })
        return context
    
class SellerDetailView(generic.DetailView):
    model = Seller
    template_name = "my_ygo_cards/seller_detail.html"

    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        seller = self.get_object()
        lots = seller.lot_set.prefetch_related("cards") # type: ignore
        context["lots"] = lots
        context["seller"].total_cards = sum(lot.cards.count() for lot in lots)
        return context