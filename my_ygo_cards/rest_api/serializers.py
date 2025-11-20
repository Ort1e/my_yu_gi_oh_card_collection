# serializers.py
from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers
from ..models import Card, CardCategory, CardData, DeckVersion, Unite, AdvancedBanList, BanListEntry

class UniteSerializer(serializers.ModelSerializer):
    class Meta:
        model = Unite
        fields = ["price"]

class BanListEntryForCardDataSerializer(serializers.ModelSerializer):
    ban_list_date = serializers.DateField(source='ban_list.date')
    ban_list_id = serializers.IntegerField(source='ban_list.id')

    class Meta:
        model = BanListEntry
        fields = ['ban_list_date', 'status', 'ban_list_id']

class CardDataSerializer(serializers.ModelSerializer):
    ban_statuses = BanListEntryForCardDataSerializer(source='ban_list_entries', many=True, read_only=True)
    class Meta:
        model = CardData
        fields = ["en_name", "ygopro_id", "card_type", "json_data", "image_url", "ban_statuses"]

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

    @extend_schema_field(serializers.BooleanField())
    def get_is_proxy(self, obj):
        return obj.is_proxy

    @extend_schema_field(serializers.FloatField(allow_null=True))
    def get_unite_price(self, obj):
        unite = Unite.objects.filter(card=obj).order_by('-id').first()
        if unite:
            return unite.price
        return None
    
class BanListEntrySerializer(serializers.ModelSerializer):
    card_data = CardDataSerializer(read_only=True)

    class Meta:
        model = BanListEntry
        fields = [
            'id',
            'card_data',
            'status',
        ]
        read_only_fields = fields
        

class AdvancedBanListSerializer(serializers.ModelSerializer):
    entries = BanListEntrySerializer(many=True, read_only=True)

    class Meta:
        model = AdvancedBanList
        fields = [
            'id',
            'date',
            'entries',
        ]
        read_only_fields = fields

class DeckVersionSerializer(serializers.ModelSerializer):
    name = serializers.SerializerMethodField()
    main_deck = CardSerializer(many=True, read_only=True)
    extra_deck = CardSerializer(many=True, read_only=True)
    side_deck = CardSerializer(many=True, read_only=True)
    ydke_with_proxies = serializers.SerializerMethodField()
    ydke_without_proxies = serializers.SerializerMethodField()
    ydke_only_proxies = serializers.SerializerMethodField()
    ban_list = AdvancedBanListSerializer(read_only=True)

    class Meta:
        model = DeckVersion
        fields = ['id', 'name', 'version_name', 'main_deck', 'extra_deck', 'side_deck', 
                  'ydke_with_proxies', 'ydke_without_proxies', 'ydke_only_proxies', 'ban_list']
    
    @extend_schema_field(serializers.CharField())
    def get_ydke_with_proxies(self, obj : DeckVersion):
        return obj.ydke_with_proxies
    
    @extend_schema_field(serializers.CharField())
    def get_ydke_without_proxies(self, obj : DeckVersion):
        return obj.ydke_without_proxies
    
    @extend_schema_field(serializers.CharField())
    def get_ydke_only_proxies(self, obj : DeckVersion):
        return obj.ydke_only_proxies
    
    @extend_schema_field(serializers.CharField())
    def get_name(self, obj):
        return obj.name

class DeckVersionUpdateSerializer(serializers.ModelSerializer):
    main_deck = serializers.ListField(child=serializers.IntegerField(), required=False)
    extra_deck = serializers.ListField(child=serializers.IntegerField(), required=False)
    side_deck = serializers.ListField(child=serializers.IntegerField(), required=False)
    ban_list_id = serializers.IntegerField(required=False, allow_null=True)

    class Meta:
        model = DeckVersion
        fields = ['version_name', 'main_deck', 'extra_deck', 'side_deck', 'ban_list_id']

class DeckYdkeImportSerializer(serializers.Serializer):
    ydke_url = serializers.CharField(required=True)
    name = serializers.CharField(required=False, default="New Deck")

class CardCategorySerializer(serializers.ModelSerializer):
    assigned = serializers.BooleanField(default=False)
    is_conditional = serializers.SerializerMethodField()

    class Meta:
        model = CardCategory
        fields = ["id", "name", "assigned", "is_conditional"]

    @extend_schema_field(serializers.BooleanField())
    def get_is_conditional(self, obj):
        return hasattr(obj, 'cardconditionalcategory')

class CardCategoryAssignmentSerializer(serializers.Serializer):
    category_id = serializers.IntegerField()
    assigned = serializers.BooleanField()



