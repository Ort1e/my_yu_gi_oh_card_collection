
from django import forms
from django.urls import reverse_lazy
from django.views import generic
from django.contrib.auth.mixins import LoginRequiredMixin

from django.shortcuts import redirect
from my_ygo_cards.models import  Deck, DeckVersion

class DeckVersionCreateForm(forms.ModelForm):
    class Meta:
        model = DeckVersion
        fields = ['version_name']  # ONLY this field

class AddDeckView(LoginRequiredMixin, generic.CreateView):
    model = Deck
    fields = ['name', 'description']
    template_name = "my_ygo_cards/add_deck.html"
    success_url = reverse_lazy("decks")
    

    def get(self, request, *args, **kwargs):
        self.object = None
        deck_form = self.get_form()
        version_form = DeckVersionCreateForm()
        return self.render_to_response(self.get_context_data(
            form=deck_form,
            version_form=version_form,
        ))

    def post(self, request, *args, **kwargs):
        self.object = None
        deck_form = self.get_form()
        version_form = DeckVersionCreateForm(request.POST)

        if deck_form.is_valid() and version_form.is_valid():
            return self.forms_valid(deck_form, version_form)
        return self.forms_invalid(deck_form, version_form)

    def forms_valid(self, deck_form, version_form):
        # Save deck
        deck = deck_form.save()

        # Save first version
        version = version_form.save(commit=False)
        version.deck = deck
        version.save()

        return redirect(self.success_url)

    def forms_invalid(self, deck_form, version_form):
        return  self.render_to_response({
            "form": deck_form,
            "version_form": version_form,
        })

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
            "adding_url_name": "add_deck",
        })
        return context
    
class DeckDetailView(generic.DetailView):
    model = Deck
    template_name = "my_ygo_cards/deck_detail.html"

    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        context["versions"] = self.get_object().versions.all().order_by("-version_name") # type: ignore
        return context