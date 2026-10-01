const STORAGE_KEY = "kadi-atlas-language";
const SUPPORTED = new Set(["tr", "en"]);

const messages = {
  tr: {
    "meta.title": "Kadı Atlası · Boğaziçi Üniversitesi",
    "meta.description": "Osmanlı kadı atamalarını zaman, mekân ve kaynaklar üzerinden incelemek için tarihsel araştırma platformu.",
    "accessibility.skip": "İçeriğe geç",
    "brand.home": "Kadı Atlası ana sayfa",
    "brand.name": "Kadı Atlası",
    "brand.university": "Boğaziçi Üniversitesi",
    "nav.main": "Ana menü",
    "nav.map": "Harita",
    "nav.sources": "Kaynaklar",
    "nav.about": "Yöntem",
    "theme.toggle": "Temayı değiştir",
    "status.loading": "Veri yükleniyor",
    "status.records": "{{count}} kayıt",
    "status.connectionError": "Bağlantı hatası",

    "common.all": "Tümü",
    "common.close": "Kapat",
    "common.loading": "Yükleniyor…",
    "common.place": "Yer",
    "common.unknownPlace": "Bilinmeyen yer",
    "common.noCoordinates": "Koordinat yok",

    "search.eyebrow": "Ara",
    "search.person": "Kadı",
    "search.personLabel": "Kadı adı ara",
    "search.personPlaceholder": "İsim ara…",
    "search.personResults": "Kadı arama sonuçları",
    "search.personHint": "Birden fazla kadı seçerek güzergâhlarını karşılaştırabilirsiniz.",
    "search.placePlaceholder": "Yer adı ara…",
    "search.placeResults": "Yer arama sonuçları",
    "search.searching": "Aranıyor…",
    "search.noResults": "Sonuç bulunamadı",

    "persons.selected": "Seçili kadılar",
    "persons.remove": "{{name}} seçimini kaldır",
    "persons.limit": "En fazla {{count}} kadı aynı anda karşılaştırılabilir.",
    "persons.onMap": "Haritada",
    "persons.showRoute": "Güzergâhı göster",

    "time.eyebrow": "Zaman",
    "time.range": "Tarih aralığı",
    "time.start": "Başlangıç",
    "time.end": "Bitiş",
    "time.note": "Yıllar kaynak metindeki tarihten çıkarılmış yaklaşık değerlerdir; takvim henüz belirlenmemiştir.",
    "time.allPeriod": "Tüm dönem",
    "time.showAll": "Tümünü göster",
    "time.rangeLabel": "{{from}}–{{to}}",
    "time.until": "{{from}}–{{year}}",
    "time.unknown": "Tarihsiz",
    "time.undated": "Tarih aralığı yok",
    "time.interval": "{{from}}–{{to}}",
    "time.after": "{{year}} sonrası",
    "time.before": "{{year}} öncesi",
    "timeline.year": "Harita tarihi",
    "timeline.play": "Zaman akışını oynat",
    "timeline.stop": "Zaman akışını durdur",
    "timeline.playShort": "Oynat",
    "timeline.stopShort": "Durdur",

    "appointments.eyebrow": "Atama",
    "appointments.count": "{{count}} atama",
    "filters.aria": "Araştırma filtreleri",
    "filters.title": "Filtreler",
    "filters.degree": "Derece",
    "filters.positionType": "Atama türü",
    "filters.place": "Yer",
    "filters.removePlace": "Yer filtresini kaldır",
    "actions.resetFilters": "Filtreleri sıfırla",
    "actions.clearAll": "Tümünü kaldır",
    "actions.retry": "Yeniden dene",
    "actions.focusRoute": "Güzergâha odaklan",
    "actions.filterByPlace": "Haritayı bu yere göre süz",
    "actions.removePlaceFilter": "Yer filtresini kaldır",

    "dataset.eyebrow": "Veri kümesi",
    "dataset.records": "Kayıt",
    "dataset.persons": "Kadı",
    "dataset.places": "Yer",
    "dataset.geocoded": "Koordinatlı yer",
    "dataset.years": "Yıl aralığı",

    "map.eyebrow": "Harita görünümü",
    "map.routeEyebrow": "Güzergâh",
    "map.title": "Atama hareketleri",
    "map.placesTitle": "Atama yerleri",
    "map.subtitle": "Koordinatı bulunan kayıtlar zaman ve mekân içinde gösterilir.",
    "map.filteredBy": "Süzgeç: {{filters}}",
    "map.years": "{{from}}–{{to}}",
    "map.routeSubtitle": "İlk atamadan son atamaya, kayıt sırasıyla",
    "map.routeWindow": "{{from}}–{{to}} arasındaki atamalar",
    "map.compareTitle": "{{count}} kadı karşılaştırılıyor",
    "map.viewMode": "Harita görünümü",
    "map.flows": "Akışlar",
    "map.places": "Yerler",
    "map.modeDisabled": "Kadı güzergâhları gösterilirken kullanılamaz",
    "map.aria": "Tarihsel kadı atama haritası",
    "map.zoom": "Yakınlaştırma",
    "map.zoomIn": "Yakınlaştır",
    "map.zoomOut": "Uzaklaştır",
    "map.fit": "Haritayı verilere sığdır",
    "map.loading": "Harita hazırlanıyor",
    "map.legend": "Harita açıklaması",
    "map.legendFlow": "Atama yönü · kalınlık kayıt sayısını gösterir",
    "map.legendPoints": "Büyüklük atama sayısını gösterir",
    "map.legendOrder": "Güzergâhtaki sıra",
    "map.flowCount": "{{count}} akış",
    "map.flowCountPartial": "En yoğun {{shown}} akış · toplam {{total}}",
    "map.placeCount": "{{count}} yer",
    "map.recordCount": "{{count}} kayıt",
    "map.flowAria": "{{origin}} → {{destination}}, {{count}} atama",
    "map.placeTitle": "{{name}} · {{arrivals}} varış, {{departures}} ayrılış",
    "map.emptyTitle": "Bu süzgeçlerle gösterilecek kayıt yok",
    "map.emptyText": "Tarih aralığını genişletin veya filtreleri sıfırlayın.",
    "map.noGeoTitle": "Henüz koordinat verisi yok",
    "map.noGeoText": "Haritada yalnızca koordinatı girilmiş yerler görünür.",
    "map.routeNoGeoTitle": "Bu güzergâhın yerlerinin koordinatı yok",
    "map.routeNoGeoText": "Kayıtlar sağdaki panelde listelenir; koordinatlar eklendiğinde haritada görünür.",
    "map.routeOutsideTitle": "Bu tarih aralığında atama yok",
    "map.routeOutsideText": "Zaman çizelgesini ilerletin veya tarih aralığını genişletin.",

    "details.aria": "Seçili kayıt detayları",
    "details.eyebrow": "Seçim",
    "details.overview": "Genel görünüm",
    "details.empty": "Bir kadı, yer, akış veya kaynak seçildiğinde ayrıntılar burada görünür.",
    "details.close": "Detay panelini kapat",
    "details.open": "Detaylar",
    "details.person": "Kadı",
    "details.place": "Yer",
    "details.flow": "Akış",
    "details.source": "Kaynak",
    "details.record": "Kayıt",
    "details.route": "Güzergâh",
    "details.appointments": "Atamalar",
    "details.persons": "Kadılar",
    "details.names": "Adlar",
    "details.wikidata": "Wikidata",
    "details.placesVisited": "Yer",
    "details.firstRecord": "İlk kayıt",
    "details.lastRecord": "Son kayıt",
    "details.arrivals": "Varış",
    "details.departures": "Ayrılış",
    "details.distinctPersons": "Kadı",
    "details.window": "Tarih aralığı",
    "details.provisionalNote": "Aynı yazılan adlar geçici olarak gruplanmıştır.",
    "details.flowSubtitle": "Bu iki yer arasındaki atama kayıtları",
    "details.loadingPerson": "Kadı yükleniyor…",
    "details.loadingPlace": "Yer yükleniyor…",
    "details.noAppointments": "Kayıtlı atama yok.",
    "details.noPersons": "Bu yere bağlı kadı bulunamadı.",
    "details.noNames": "Kayıtlı tarihsel ad yok.",
    "details.personsCapped": "En etkin 50 kadı gösteriliyor.",
    "details.listCapped": "İlk {{shown}} kayıt gösteriliyor · toplam {{total}}",
    "details.arrivedCount": "{{count}} kez geldi",
    "details.leftCount": "{{count}} kez ayrıldı",

    "route.first": "İlk atama",
    "route.last": "Son atama",
    "route.start": "Başlangıç",
    "route.restart": "Yeni başlangıç",
    "route.arrived": "{{year}} · varış",
    "route.gap": "bağlantısız",
    "route.gapRow": "Kayıt boşluğu: önceki atamanın varış yeri bu atamanın çıkış yerinden farklı.",
    "route.help": "Sıralama önce tarihe göre yapılır; aynı yıl veya tarihsiz kayıtlarda eski yer → yeni yer bağlantısı izlenir.",
    "route.showOnMap": "Haritada göster",
    "route.departedAt": "{{year}} · görevden ayrılış",
    "route.departedRow": "{{place}} · görevden ayrılış",
    "route.leftPost": "{{year}} · görevden ayrıldı, halefi {{successor}}",
    "route.successor": "Halefi: {{name}}",
    "route.predecessor": "Selefi: {{name}}",

    "resolution.provisional": "Geçici",
    "resolution.under_review": "İnceleniyor",
    "resolution.confirmed": "Doğrulanmış",
    "resolution.split_needed": "Ayrıştırılmalı",
    "calendar.unknown": "Belirlenmemiş",
    "calendar.hijri": "Hicrî",
    "calendar.rumi": "Rûmî",
    "calendar.julian": "Jülyen",
    "calendar.gregorian": "Miladi",

    "record.date": "Tarih",
    "record.calendar": "Takvim",
    "record.oldKadi": "Eski kadı",
    "record.newKadi": "Yeni kadı",
    "record.oldPlace": "Eski yer",
    "record.newPlace": "Yeni yer",
    "record.period": "Dönem",
    "record.salary": "Maaş",
    "record.region": "Bölge",
    "record.document": "Belge",
    "record.folio": "Varak",
    "salary.change": "{{old}} → {{new}}",

    "source.record": "Kayıt",
    "source.text": "Kaynak metin",
    "source.none": "Kaynak metin seçilmedi.",
    "source.notFound": "Kaynak metin yok.",
    "source.folio": "Varak {{value}}",
    "source.certificate": "Sertifika",
    "source.raw": "Ham kayıt (tablodaki satır)",

    "sources.eyebrow": "Arşiv",
    "sources.title": "Kaynak metinleri",
    "sources.subtitle": "Kaynak metinlerde tam metin arama. Bir kaydı açarak ilgili atamayı görün.",
    "sources.searchLabel": "Kaynaklarda ara",
    "sources.placeholder": "Örn. kaza tevcih",
    "sources.help": "Kelimelerin tümü aranır; \"tırnak içindeki\" ifade aynen aranır, -kelime hariç tutulur.",
    "sources.more": "Daha fazla göster",
    "sources.count": "{{count}} kaynak",
    "sources.countMatches": "{{count}} eşleşme",
    "sources.noResults": "“{{q}}” için kaynak bulunamadı.",
    "sources.none": "Henüz kaynak metin yok.",

    "guide.searchTitle": "Kadı arayın",
    "guide.search": "Soldaki arama kutusuna bir ad yazın (kısayol: /). Seçilen kadının güzergâhı haritada kendi rengiyle çizilir.",
    "guide.compareTitle": "Karşılaştırın",
    "guide.compare": "Birden fazla kadı ekleyin; her biri ayrı renk alır, renk ilk atamadan son atamaya koyulaşır.",
    "guide.timeTitle": "Zamanda gezinin",
    "guide.time": "Alttaki zaman çizelgesini sürükleyin veya oynatın; harita seçilen yıla kadarki atamaları gösterir.",
    "guide.sourceTitle": "Kaynağa inin",
    "guide.source": "Bir atamaya tıklayınca kaynak metni ve ham kayıt açılır. Kaynaklar sekmesinde tam metin arama yapabilirsiniz.",

    "about.eyebrow": "Yöntem",
    "about.title": "Veriler nasıl okunmalı?",
    "about.link": "Yöntem notlarını okuyun →",
    "about.intro": "Kadı Atlası, Osmanlı kadı atama kayıtlarını harita, zaman ve kaynak metin üzerinden incelemek için hazırlanmış bir araştırma aracıdır. Aşağıdaki notlar verinin sınırlarını açıklar.",
    "about.yearsTitle": "Tarihler",
    "about.years": "Kaynaktaki tarih ifadesi aynen korunur. Filtreleme için metindeki ilk 3–4 haneli sayı yıl olarak kullanılır; takvim (hicrî, rûmî…) henüz belirlenmemiştir ve bu sayı kesin bir miladi yıl değildir.",
    "about.identityTitle": "Kişiler ve yerler",
    "about.identity": "Aynı şekilde yazılan adlar geçici olarak tek kişi veya tek yer altında toplanır. Bu bir kimlik tespiti değildir; farklı kişiler aynı adı taşıyabilir.",
    "about.routeTitle": "Güzergâhlar",
    "about.route": "Bir kadının atamaları önce yıla göre sıralanır. Aynı yıl içindeki veya tarihsiz kayıtlarda eski yer → yeni yer bağlantısı izlenir (A→B, B→C…). Bağlantının koptuğu yerler ayrıca işaretlenir.",
    "about.colorsTitle": "Renkler",
    "about.colors": "Her kadı kendi rengiyle çizilir: ilk atama yerinde açık, güzergâh boyunca koyulaşır. Akış görünümünde çizgi kalınlığı kayıt sayısını gösterir; oklar yönü belirtir.",
    "about.mapTitle": "Harita",
    "about.map": "Yalnızca koordinatı girilmiş yerler haritada görünür. Kıyı çizgileri ve nehirler günümüz coğrafyasıdır (Natural Earth).",

    "errors.personSearch": "Kadı araması yapılamadı.",
    "errors.placeSearch": "Yer araması yapılamadı.",
    "errors.sourceSearch": "Kaynak araması yapılamadı.",
    "errors.personLoad": "Kadı bilgisi yüklenemedi.",
    "errors.personLinkMissing": "Bağlantıdaki kadılardan bazıları bulunamadı.",
    "errors.placeLoad": "Yer bilgisi yüklenemedi.",
    "errors.flowLoad": "Bu akışın kayıtları yüklenemedi.",
    "errors.sourceLoad": "Kaynak metin yüklenemedi.",
    "errors.mapLoad": "Harita verisi alınamadı",
    "errors.dataUnavailableTitle": "Veriye erişilemiyor",
    "errors.dataUnavailableText": "Sunucu veya veritabanı bağlantısını kontrol edip yeniden deneyin.",
    "errors.invalidQuery": "Geçersiz sorgu.",
    "errors.notFound": "Kayıt bulunamadı.",
    "errors.timeout": "İstek zaman aşımına uğradı.",
    "errors.tooMany": "Çok fazla istek gönderildi.",
    "errors.internal": "Sunucuda beklenmeyen bir hata oluştu.",
    "errors.unavailable": "Servis şu anda kullanılamıyor.",
    "errors.requestFailed": "İstek başarısız oldu ({{status}}).",
    "errors.invalidResponse": "Sunucu geçersiz bir yanıt döndürdü.",
    "errors.network": "Sunucuya ulaşılamadı.",
    "errors.aborted": "İstek iptal edildi.",
    "validation.yearOrder": "Bitiş yılı başlangıç yılından küçük olamaz.",
    "validation.yearInteger": "Yıl tam sayı olmalıdır."
  },

  en: {
    "meta.title": "Kadı Atlas · Boğaziçi University",
    "meta.description": "A historical research platform for exploring Ottoman kadı appointments across time, space and sources.",
    "accessibility.skip": "Skip to content",
    "brand.home": "Kadı Atlas home",
    "brand.name": "Kadı Atlas",
    "brand.university": "Boğaziçi University",
    "nav.main": "Main menu",
    "nav.map": "Map",
    "nav.sources": "Sources",
    "nav.about": "Method",
    "theme.toggle": "Toggle theme",
    "status.loading": "Loading data",
    "status.records": "{{count}} records",
    "status.connectionError": "Connection error",

    "common.all": "All",
    "common.close": "Close",
    "common.loading": "Loading…",
    "common.place": "Place",
    "common.unknownPlace": "Unknown place",
    "common.noCoordinates": "No coordinates",

    "search.eyebrow": "Search",
    "search.person": "Kadı",
    "search.personLabel": "Search kadı names",
    "search.personPlaceholder": "Search names…",
    "search.personResults": "Kadı search results",
    "search.personHint": "Select several kadıs to compare their itineraries.",
    "search.placePlaceholder": "Search places…",
    "search.placeResults": "Place search results",
    "search.searching": "Searching…",
    "search.noResults": "No results",

    "persons.selected": "Selected kadıs",
    "persons.remove": "Remove {{name}}",
    "persons.limit": "Up to {{count}} kadıs can be compared at once.",
    "persons.onMap": "On map",
    "persons.showRoute": "Show itinerary",

    "time.eyebrow": "Time",
    "time.range": "Date range",
    "time.start": "From",
    "time.end": "To",
    "time.note": "Years are approximate values taken from the source date; the calendar has not been determined yet.",
    "time.allPeriod": "Whole period",
    "time.showAll": "Show all",
    "time.rangeLabel": "{{from}}–{{to}}",
    "time.until": "{{from}}–{{year}}",
    "time.unknown": "Undated",
    "time.undated": "No date range",
    "time.interval": "{{from}}–{{to}}",
    "time.after": "after {{year}}",
    "time.before": "before {{year}}",
    "timeline.year": "Map year",
    "timeline.play": "Play timeline",
    "timeline.stop": "Stop timeline",
    "timeline.playShort": "Play",
    "timeline.stopShort": "Stop",

    "appointments.eyebrow": "Appointment",
    "appointments.count": "{{count}} appointments",
    "filters.aria": "Research filters",
    "filters.title": "Filters",
    "filters.degree": "Degree",
    "filters.positionType": "Appointment type",
    "filters.place": "Place",
    "filters.removePlace": "Remove place filter",
    "actions.resetFilters": "Reset filters",
    "actions.clearAll": "Remove all",
    "actions.retry": "Try again",
    "actions.focusRoute": "Focus itinerary",
    "actions.filterByPlace": "Filter map by this place",
    "actions.removePlaceFilter": "Remove place filter",

    "dataset.eyebrow": "Dataset",
    "dataset.records": "Records",
    "dataset.persons": "Kadıs",
    "dataset.places": "Places",
    "dataset.geocoded": "Geocoded places",
    "dataset.years": "Year range",

    "map.eyebrow": "Map view",
    "map.routeEyebrow": "Itinerary",
    "map.title": "Appointment movements",
    "map.placesTitle": "Appointment places",
    "map.subtitle": "Records with coordinates are shown across time and space.",
    "map.filteredBy": "Filtered by: {{filters}}",
    "map.years": "{{from}}–{{to}}",
    "map.routeSubtitle": "From the first appointment to the last, in record order",
    "map.routeWindow": "Appointments between {{from}} and {{to}}",
    "map.compareTitle": "Comparing {{count}} kadıs",
    "map.viewMode": "Map view",
    "map.flows": "Flows",
    "map.places": "Places",
    "map.modeDisabled": "Not available while itineraries are shown",
    "map.aria": "Historical kadı appointment map",
    "map.zoom": "Zoom",
    "map.zoomIn": "Zoom in",
    "map.zoomOut": "Zoom out",
    "map.fit": "Fit map to data",
    "map.loading": "Preparing map",
    "map.legend": "Map legend",
    "map.legendFlow": "Direction of appointment · width shows record count",
    "map.legendPoints": "Size shows number of appointments",
    "map.legendOrder": "Order in itinerary",
    "map.flowCount": "{{count}} flows",
    "map.flowCountPartial": "Top {{shown}} flows · {{total}} in total",
    "map.placeCount": "{{count}} places",
    "map.recordCount": "{{count}} records",
    "map.flowAria": "{{origin}} → {{destination}}, {{count}} appointments",
    "map.placeTitle": "{{name}} · {{arrivals}} arrivals, {{departures}} departures",
    "map.emptyTitle": "No records match these filters",
    "map.emptyText": "Widen the date range or reset the filters.",
    "map.noGeoTitle": "No coordinate data yet",
    "map.noGeoText": "Only places with coordinates appear on the map.",
    "map.routeNoGeoTitle": "This itinerary's places have no coordinates",
    "map.routeNoGeoText": "The records are listed in the panel; they will appear on the map once coordinates are added.",
    "map.routeOutsideTitle": "No appointments in this date range",
    "map.routeOutsideText": "Move the timeline forward or widen the date range.",

    "details.aria": "Selected record details",
    "details.eyebrow": "Selection",
    "details.overview": "Overview",
    "details.empty": "Select a kadı, place, flow or source to see details here.",
    "details.close": "Close details panel",
    "details.open": "Details",
    "details.person": "Kadı",
    "details.place": "Place",
    "details.flow": "Flow",
    "details.source": "Source",
    "details.record": "Record",
    "details.route": "Itinerary",
    "details.appointments": "Appointments",
    "details.persons": "Kadıs",
    "details.names": "Names",
    "details.wikidata": "Wikidata",
    "details.placesVisited": "Places",
    "details.firstRecord": "First record",
    "details.lastRecord": "Last record",
    "details.arrivals": "Arrivals",
    "details.departures": "Departures",
    "details.distinctPersons": "Kadıs",
    "details.window": "Date range",
    "details.provisionalNote": "Identically spelled names are grouped provisionally.",
    "details.flowSubtitle": "Appointment records between these two places",
    "details.loadingPerson": "Loading kadı…",
    "details.loadingPlace": "Loading place…",
    "details.noAppointments": "No recorded appointments.",
    "details.noPersons": "No kadıs linked to this place.",
    "details.noNames": "No recorded historical names.",
    "details.personsCapped": "Showing the 50 most active kadıs.",
    "details.listCapped": "Showing the first {{shown}} records · {{total}} in total",
    "details.arrivedCount": "arrived {{count}}×",
    "details.leftCount": "left {{count}}×",

    "route.first": "First appointment",
    "route.last": "Last appointment",
    "route.start": "Start",
    "route.restart": "New start",
    "route.arrived": "{{year}} · arrival",
    "route.gap": "not linked",
    "route.gapRow": "Record gap: the previous appointment's destination differs from this one's origin.",
    "route.help": "Ordered by year first; same-year and undated records follow the old place → new place chain.",
    "route.showOnMap": "Show on map",
    "route.departedAt": "{{year}} · left post",
    "route.departedRow": "{{place}} · left post",
    "route.leftPost": "{{year}} · left post, succeeded by {{successor}}",
    "route.successor": "Succeeded by {{name}}",
    "route.predecessor": "Replaced {{name}}",

    "resolution.provisional": "Provisional",
    "resolution.under_review": "Under review",
    "resolution.confirmed": "Confirmed",
    "resolution.split_needed": "Needs splitting",
    "calendar.unknown": "Not determined",
    "calendar.hijri": "Hijri",
    "calendar.rumi": "Rumi",
    "calendar.julian": "Julian",
    "calendar.gregorian": "Gregorian",

    "record.date": "Date",
    "record.calendar": "Calendar",
    "record.oldKadi": "Outgoing kadı",
    "record.newKadi": "Incoming kadı",
    "record.oldPlace": "Old place",
    "record.newPlace": "New place",
    "record.period": "Period",
    "record.salary": "Salary",
    "record.region": "Region",
    "record.document": "Document",
    "record.folio": "Folio",
    "salary.change": "{{old}} → {{new}}",

    "source.record": "Record",
    "source.text": "Source text",
    "source.none": "No source selected.",
    "source.notFound": "No source text.",
    "source.folio": "Folio {{value}}",
    "source.certificate": "Certificate",
    "source.raw": "Raw record (spreadsheet row)",

    "sources.eyebrow": "Archive",
    "sources.title": "Source texts",
    "sources.subtitle": "Full-text search across the source texts. Open a record to see its appointment.",
    "sources.searchLabel": "Search sources",
    "sources.placeholder": "e.g. kaza tevcih",
    "sources.help": "All words are required; \"quoted phrases\" match exactly, -word excludes.",
    "sources.more": "Show more",
    "sources.count": "{{count}} sources",
    "sources.countMatches": "{{count}} matches",
    "sources.noResults": "No sources found for “{{q}}”.",
    "sources.none": "No source texts yet.",

    "guide.searchTitle": "Search a kadı",
    "guide.search": "Type a name into the search box on the left (shortcut: /). The kadı's itinerary is drawn on the map in its own colour.",
    "guide.compareTitle": "Compare",
    "guide.compare": "Add several kadıs; each gets its own colour, darkening from the first appointment to the last.",
    "guide.timeTitle": "Move through time",
    "guide.time": "Drag or play the timeline below the map; it shows appointments up to the selected year.",
    "guide.sourceTitle": "Go to the source",
    "guide.source": "Click an appointment to open its source text and raw record. The Sources tab offers full-text search.",

    "about.eyebrow": "Method",
    "about.title": "How to read the data",
    "about.link": "Read the method notes →",
    "about.intro": "Kadı Atlas is a research tool for exploring Ottoman kadı appointment records through maps, time and source texts. These notes explain the limits of the data.",
    "about.yearsTitle": "Dates",
    "about.years": "The date as written in the source is kept verbatim. For filtering, the first 3–4 digit number in it is used as the year; the calendar (Hijri, Rumi…) has not been determined, so this number is not an exact Gregorian year.",
    "about.identityTitle": "People and places",
    "about.identity": "Identically spelled names are grouped provisionally under one person or place. This is not an identification; different people may share a name.",
    "about.routeTitle": "Itineraries",
    "about.route": "A kadı's appointments are ordered by year first. Within a year, or for undated records, the old place → new place chain is followed (A→B, B→C…). Places where the chain breaks are marked.",
    "about.colorsTitle": "Colours",
    "about.colors": "Each kadı is drawn in their own colour: light at the first appointment, darkening along the itinerary. In the flow view, line width shows record count and arrows show direction.",
    "about.mapTitle": "Map",
    "about.map": "Only places with coordinates appear on the map. Coastlines and rivers are present-day geography (Natural Earth).",

    "errors.personSearch": "Kadı search failed.",
    "errors.placeSearch": "Place search failed.",
    "errors.sourceSearch": "Source search failed.",
    "errors.personLoad": "Could not load kadı data.",
    "errors.personLinkMissing": "Some kadıs from the link could not be found.",
    "errors.placeLoad": "Could not load place data.",
    "errors.flowLoad": "Could not load this flow's records.",
    "errors.sourceLoad": "Could not load the source text.",
    "errors.mapLoad": "Could not load map data",
    "errors.dataUnavailableTitle": "Cannot reach the data",
    "errors.dataUnavailableText": "Check the server or database connection and try again.",
    "errors.invalidQuery": "Invalid query.",
    "errors.notFound": "Record not found.",
    "errors.timeout": "Request timed out.",
    "errors.tooMany": "Too many requests.",
    "errors.internal": "The server encountered an unexpected error.",
    "errors.unavailable": "The service is currently unavailable.",
    "errors.requestFailed": "Request failed ({{status}}).",
    "errors.invalidResponse": "The server returned an invalid response.",
    "errors.network": "Could not reach the server.",
    "errors.aborted": "Request cancelled.",
    "validation.yearOrder": "The end year cannot be earlier than the start year.",
    "validation.yearInteger": "Years must be whole numbers."
  }
};

let locale = detectLocale();

export function t(key, vars = {}) {
  const template =
    messages[locale]?.[key] ??
    messages.tr[key] ??
    key;

  return template.replace(/\{\{(\w+)\}\}/g, (_, name) => {
    const value = vars[name];
    return value === undefined || value === null ? "" : String(value);
  });
}

export function getLocale() {
  return locale;
}

export function setLocale(next) {
  const normalized = SUPPORTED.has(next) ? next : "tr";

  if (normalized === locale) {
    applyLocale();
    return;
  }

  locale = normalized;

  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // The browser has rejected diplomacy.
  }

  applyLocale();

  window.dispatchEvent(
    new CustomEvent("kadi:localechange", {
      detail: { locale }
    })
  );
}

export function numberLocale() {
  return locale === "en" ? "en-US" : "tr-TR";
}


/* Two languages. Zero frameworks. Civilization survives. */

function init() {
  applyLocale();

  const toggle = document.getElementById("language-toggle");

  toggle?.addEventListener("click", () => {
    setLocale(locale === "tr" ? "en" : "tr");
  });
}

function applyLocale() {
  document.documentElement.lang = locale;

  document.querySelectorAll("[data-i18n]").forEach((element) => {
    element.textContent = t(element.dataset.i18n);
  });

  document.querySelectorAll("[data-i18n-placeholder]").forEach((element) => {
    element.setAttribute(
      "placeholder",
      t(element.dataset.i18nPlaceholder)
    );
  });

  document.querySelectorAll("[data-i18n-aria-label]").forEach((element) => {
    element.setAttribute(
      "aria-label",
      t(element.dataset.i18nAriaLabel)
    );
  });

  document.querySelectorAll("[data-i18n-title]").forEach((element) => {
    element.setAttribute(
      "title",
      t(element.dataset.i18nTitle)
    );
  });

  document.querySelectorAll("[data-i18n-content]").forEach((element) => {
    element.setAttribute(
      "content",
      t(element.dataset.i18nContent)
    );
  });

  const toggle = document.getElementById("language-toggle");
  const label = document.getElementById("language-toggle-label");

  if (label) {
    label.textContent = locale === "tr" ? "EN" : "TR";
  }

  if (toggle) {
    if (locale === "tr") {
      toggle.setAttribute("aria-label", "Switch to English");
      toggle.setAttribute("title", "English");
    } else {
      toggle.setAttribute("aria-label", "Türkçeye geç");
      toggle.setAttribute("title", "Türkçe");
    }
  }
}

function detectLocale() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);

    if (SUPPORTED.has(stored)) {
      return stored;
    }
  } catch {
    // Storage has declared neutrality.
  }

  return navigator.language?.toLowerCase().startsWith("en")
    ? "en"
    : "tr";
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init, { once: true });
} else {
  init();
}
