from pathlib import Path
from urllib.parse import urlencode

from django import forms
from django.shortcuts import redirect
from django.views import generic
from django.contrib.auth.mixins import LoginRequiredMixin
from django.views.generic.edit import CreateView
from django.urls import reverse_lazy
from django.core.files import File

from my_ygo_cards.shipment_reader.cardmarket import CardmarketShipmentReader

from my_ygo_cards.shipment_reader.shipment_file_handler import SHIPMENT_UPLOAD_FOLDER, ShipmentFileHandler

from .lot_form import LotForm, SellerForm, UniteForm, UploadShipmentForm

from my_ygo_cards.models import Card, Lot, Seller, SellerSource, Unite

class LotListView(generic.ListView):
    paginate_by = 10  # Number of lots per page
    model = Lot
    template_name = 'my_ygo_cards/list_base.html'
    context_object_name = 'object_list'  # important: expected by list_base.html

    def get_queryset(self):
        queryset = Lot.objects.all()

        seller_name = self.request.GET.get("seller_name")
        is_sold = self.request.GET.get("sold")

        if seller_name:
            queryset = queryset.filter(seller__name__icontains=seller_name)

        if is_sold == 'true':
            queryset = queryset.filter(lot_type=Lot.SALE)
        elif is_sold == 'false':
            queryset = queryset.filter(lot_type=Lot.PURCHASE)

        return queryset.distinct().order_by('buy_date', 'id')


    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)

        # Build querystring excluding 'page' parameter (keep it in the links)
        get_params = self.request.GET.copy()
        get_params.pop('page', None)
        context['querystring'] = urlencode(get_params)


        
        context.update({
            'page_title': 'All Lots',
            'item_template': 'my_ygo_cards/partials/one_lot_list.html',
            'include_filter': 'my_ygo_cards/partials/lot_filter.html',
            'object_name': 'lot',
            'list_length': len(Lot.objects.all()),
            "adding_url_name": "add_lot",
        })
        return context

class LotDetailView(generic.DetailView):
    model = Lot
    template_name = 'my_ygo_cards/lot_detail.html'
    context_object_name = 'lot'

    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        lot = self.get_object()
        total_price = sum(unite.price for unite in lot.unite_set.all() if unite.price) # type: ignore
        context['total_price'] = total_price
        context['unite_form'] = UniteForm()
        return context
    
    def post(self, request, *args, **kwargs):
        lot = self.get_object()
        unite_form = UniteForm(request.POST)
        if unite_form.is_valid():
            quantity = unite_form.cleaned_data.get("quantity", 1)
            print(f"nb unite : {quantity}")
            for _ in range(quantity):
                # Create a fresh Card
                card = Card.objects.create(
                    name=unite_form.cleaned_data["card_name"],
                    en_name=unite_form.cleaned_data.get("card_en_name") or unite_form.cleaned_data["card_name"],
                    code=unite_form.cleaned_data.get("card_code") or "",
                    last_known_status=unite_form.cleaned_data.get("card_last_known_status"),
                    is_proxy=unite_form.cleaned_data.get("card_is_proxy", False),
                )

                # Create a Unite pointing to the new card and lot
                Unite.objects.create(
                    lot=lot,
                    price=unite_form.cleaned_data["price"],
                    card=card,
                )
            return redirect('lot', pk=lot.pk)
        context = self.get_context_data()
        context['unite_form'] = unite_form
        return self.render_to_response(context)
    

class AddLotView(LoginRequiredMixin, CreateView):
    model = Lot
    form_class = LotForm
    template_name = 'my_ygo_cards/add_lot.html'
    success_url = reverse_lazy('lots')

    def get_unite_formset(self, extra):
        UniteFormSet = forms.inlineformset_factory(
            Lot, Unite, form=UniteForm,
            extra=extra, can_delete=True
        )
        return UniteFormSet
    
    def get_context_data(self, **kwargs):
        context = super().get_context_data(**kwargs)
        context.update({
            'sellers': Seller.objects.all()
        })
        return context


    def get(self, request, *args, **kwargs):
        cards_count = int(request.GET.get('cards', 1))
        form_class = self.get_form_class()
        self.object = None  # Reset the object to None for GET requests
        return self.render_to_response(self.get_context_data(
            form=form_class(prefix='lot'),
            seller_form=SellerForm(prefix='lot'),
            unite_formset=self.get_unite_formset(extra=cards_count)(prefix='lot'),
            shipment_form=UploadShipmentForm(prefix='shipment')
        ))
    
    def post(self, request, *args, **kwargs):
        cards_count = int(request.GET.get('cards', 1))
        form_class = self.get_form_class()
        form = form_class(request.POST if 'lot' in request.POST else None, prefix='lot')
        seller_form = SellerForm(request.POST if 'lot' in request.POST else None, prefix='lot')
        unite_formset = self.get_unite_formset(extra=cards_count)(request.POST if 'lot' in request.POST else None, prefix='lot')
        file_shipment_form : UploadShipmentForm
        if 'shipment' in request.POST:
            file_shipment_form = UploadShipmentForm(request.POST, request.FILES, prefix='shipment')
        else:
            file_shipment_form = UploadShipmentForm(prefix='shipment')

        self.object = None  # Reset the object to None for POST requests

        # ------------- handle file upload if needed -------------
        if file_shipment_form.is_bound and file_shipment_form.is_valid():
            shipment_file = request.FILES['shipment-shipment_file']

            file_tuple = ShipmentFileHandler(SHIPMENT_UPLOAD_FOLDER).handle_file(shipment_file)
            if not file_tuple:
                file_shipment_form.add_error(None, "Failed to process the shipment file pdf.")
                return self.render_to_response(self.get_context_data(
                    form=form,
                    seller_form=seller_form,
                    unite_formset=unite_formset,
                    shipment_form=file_shipment_form,
                ))
            file_path, file_str = file_tuple

            date_and_price = CardmarketShipmentReader.extract_dates_and_prices(file_str)
            cards = CardmarketShipmentReader.extract_cards(file_str)
            
            # seller
            # try to get existing seller by name
            seller_name = date_and_price['seller_name']
            if not seller_name:
                seller_name = date_and_price['buyer_name']

            selected_seller_id = None
            selected_seller_form : SellerForm
            try:
                seller = Seller.objects.get(name=seller_name)
                selected_seller_id = seller.pk
                selected_seller_form = SellerForm( prefix='lot')
            except Seller.DoesNotExist:
                # if seller does not exist, create a new one
                # create a new seller form with the extracted name

                seller_source = SellerSource.objects.get(name='Cardmarket')

                seller_form_preset = {
                    'name': seller_name,
                    'is_person': False,
                    'source': seller_source.pk,
                }
                selected_seller_form = SellerForm(initial=seller_form_preset, prefix='lot')
            
            lot_type = Lot.PURCHASE if date_and_price['seller_name'] is not None else Lot.SALE

            # lot
            form_preset = {
                'price': date_and_price['price'],
                'lot_type': lot_type,
                'buy_date': date_and_price['buy_date'],
                'received_date': date_and_price['received_date'],
                'no_card_price': 0,  # Assuming no card price is zero
            }
            form = form_class(initial=form_preset, prefix='lot')
            # unite
            unite_formset_type = self.get_unite_formset(extra=len(cards))
            formset = unite_formset_type(
                initial=[
                    {
                        'card_name': card['name'],
                        'card_en_name': card['name'],
                        'card_code': card['code'],
                        'card_last_known_status': card['status'],
                        'card_is_proxy': False,  # Assuming no proxies in the shipment
                        'price': card['price'],
                    } for card in cards
                ],
                prefix='lot'
            )

            return self.render_to_response(self.get_context_data(
                form=form,
                seller_form=selected_seller_form,
                selected_seller_id=selected_seller_id,
                unite_formset=formset,
                shipment_file_name= file_path.name,
            ))
        
        selected_seller_id = request.POST.get("seller_select")

        if (
            form.is_bound and form.is_valid()
            and unite_formset.is_bound and unite_formset.is_valid()
        ):
            # ----- Handle Seller -----
            seller = None
            if selected_seller_id:
                seller = Seller.objects.get(pk=selected_seller_id)
            elif seller_form.is_bound and seller_form.is_valid():
                seller_name = seller_form.cleaned_data['name']
                seller, _ = Seller.objects.get_or_create(name=seller_name)
            else:
                seller_form.add_error(None, "Seller form is invalid or not provided.")
                return self.render_to_response(self.get_context_data(
                    form=form,
                    seller_form=seller_form,
                    unite_formset=unite_formset,
                    shipment_form=file_shipment_form,
                ))

            # ----- Create and Save Lot -----
            lot = form.save(commit=False)   # don’t commit yet, we still set seller
            lot.seller = seller
            lot.save()  # now lot.pk is available

            shipment_file_name = request.POST.get("shipment_file_name")
            if shipment_file_name:
                path = Path(SHIPMENT_UPLOAD_FOLDER + shipment_file_name)
                with path.open("rb") as f:
                    name = f"shipment_{lot.pk}.pdf"
                    lot.shipment_file = File(f, name=name)
                    lot.save()

            # ----- Save Formset -----
            for unite_form in unite_formset.forms:
                quantity = unite_form.cleaned_data.get("quantity", 1)
                for _ in range(quantity):
                    # Create a fresh Card
                    card = Card.objects.create(
                        name=unite_form.cleaned_data['card_name'],
                        en_name=unite_form.cleaned_data.get('card_en_name') or unite_form.cleaned_data['card_name'],
                        code=unite_form.cleaned_data.get('card_code') or '',
                        last_known_status=unite_form.cleaned_data.get('card_last_known_status'),
                        is_proxy=unite_form.cleaned_data.get('card_is_proxy', False),
                    )

                    # Create a Unite tied to the new card and lot
                    Unite.objects.create(
                        lot=lot,
                        price=unite_form.cleaned_data['price'],
                        card=card,
                    )

            return redirect(self.success_url)

        # ------------- Validation Failed -------------

        # If any form is invalid, re-render the page with the forms and errors

        return self.render_to_response(self.get_context_data(
            form=form,
            seller_form=seller_form,
            unite_formset=unite_formset,
            shipment_form=file_shipment_form,
        ))

