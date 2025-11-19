from django import forms

from my_ygo_cards.models import Card, Lot, Seller, Unite

class SellerForm(forms.ModelForm):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        for field in self.fields.values():
            field.required = False  # Make all fields optional

    class Meta:
        model = Seller
        fields = ['name', 'is_person', 'source']

class CardForm(forms.ModelForm):
    class Meta:
        model = Card
        fields = ['name', 'code', 'last_known_status', 'is_proxy']
class UniteForm(forms.ModelForm):

    card_name = forms.CharField(max_length=255, required=True)
    card_en_name = forms.CharField(max_length=255, required=False)
    card_code = forms.CharField(max_length=20, required=False)
    card_last_known_status = forms.ChoiceField(
        choices=Card.STATUS_CHOICES, required=False
    )
    card_is_proxy = forms.BooleanField(required=False)
    quantity = forms.IntegerField(min_value=1, initial=1, required=True)  # NEW FIELD

    class Meta:
        model = Unite
        fields = ['price']  # still no 'card'



class LotForm(forms.ModelForm):
    class Meta:
        model = Lot
        fields = ['price', 'lot_type', 'buy_date', 'received_date', 'no_card_price']
        widgets = {
            'lot_type': forms.RadioSelect(choices=Lot.LOT_TYPE_CHOICES),
            'buy_date': forms.DateInput(attrs={'type': 'date'}),
            'received_date': forms.DateInput(attrs={'type': 'date'}),
        }

class UploadShipmentForm(forms.Form):
    shipment_file = forms.FileField(label='Upload Shipment File')