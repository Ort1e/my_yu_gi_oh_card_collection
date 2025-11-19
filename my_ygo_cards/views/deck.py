
from django.views import generic

from my_ygo_cards.models import  Deck

class DecksView(generic.ListView):
    paginate_by = 10
    template_name = 'my_ygo_cards/list_base.html'
    context_object_name = 'object_list'  # important: expected by list_base.html

    def get_queryset(self):
        queryset = Deck.objects.all()

        return queryset.distinct().order_by('name', 'id')
    
    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        context.update({
            'page_title': 'All Decks',
            'item_template': 'my_ygo_cards/partials/one_deck_list.html',
            'object_name': 'deck',
            'list_length': len(Deck.objects.all()),
        })
        return context
    
class DeckDetailView(generic.DetailView):
    model = Deck
    template_name = "my_ygo_cards/deck_detail.html"

    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        context["versions"] = self.get_object().versions.all().order_by("-version_name") # type: ignore
        return context