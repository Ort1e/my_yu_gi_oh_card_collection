# serializers.py
from rest_framework import serializers
from ..models import Card, CardCategory, CardData, DeckVersion, Unite

class UniteSerializer(serializers.ModelSerializer):
    class Meta:
        model = Unite
        fields = ["price"]

class CardDataSerializer(serializers.ModelSerializer):
    class Meta:
        model = CardData
        fields = ["en_name", "ygopro_id", "card_type", "json_data", "image_url"]

class CardSerializer(serializers.ModelSerializer):
    card_data = CardDataSerializer(read_only=True)
    unite_price = serializers.SerializerMethodField()
    is_proxy = serializers.SerializerMethodField()

    class Meta:
        model = Card
        fields = [
            "id",
            "name",
            "en_name",
            "code",
            "last_known_status",
            "card_data",
            "is_proxy",
            "unite_price",
        ]

    def get_is_proxy(self, obj):
        return obj.is_proxy

    def get_unite_price(self, obj):
        unite = Unite.objects.filter(card=obj).order_by('-id').first()
        if unite:
            return unite.price
        return None

class DeckVersionSerializer(serializers.ModelSerializer):
    name = serializers.SerializerMethodField()
    main_deck = CardSerializer(many=True, read_only=True)
    extra_deck = CardSerializer(many=True, read_only=True)
    side_deck = CardSerializer(many=True, read_only=True)
    ydke_with_proxies = serializers.SerializerMethodField()
    ydke_without_proxies = serializers.SerializerMethodField()
    ydke_only_proxies = serializers.SerializerMethodField()

    class Meta:
        model = DeckVersion
        fields = ['id', 'name', 'version_name', 'main_deck', 'extra_deck', 'side_deck', 
                  'ydke_with_proxies', 'ydke_without_proxies', 'ydke_only_proxies']
        
    def get_ydke_with_proxies(self, obj : DeckVersion):
        return obj.ydke_with_proxies
    
    def get_ydke_without_proxies(self, obj : DeckVersion):
        return obj.ydke_without_proxies
    
    def get_ydke_only_proxies(self, obj : DeckVersion):
        return obj.ydke_only_proxies
    
    def get_name(self, obj):
        return obj.name

class DeckVersionUpdateSerializer(serializers.ModelSerializer):
    main_deck = serializers.ListField(child=serializers.IntegerField(), required=False)
    extra_deck = serializers.ListField(child=serializers.IntegerField(), required=False)
    side_deck = serializers.ListField(child=serializers.IntegerField(), required=False)

    class Meta:
        model = DeckVersion
        fields = ['version_name', 'main_deck', 'extra_deck', 'side_deck']

class DeckYdkeImportSerializer(serializers.Serializer):
    ydke_url = serializers.CharField(required=True)
    name = serializers.CharField(required=False, default="New Deck")

class CardCategorySerializer(serializers.ModelSerializer):
    assigned = serializers.BooleanField(default=False)
    is_conditional = serializers.SerializerMethodField()

    class Meta:
        model = CardCategory
        fields = ["id", "name", "assigned", "is_conditional"]


    def get_is_conditional(self, obj):
        return hasattr(obj, 'cardconditionalcategory')

class CardCategoryAssignmentSerializer(serializers.Serializer):
    category_id = serializers.IntegerField()
    assigned = serializers.BooleanField()