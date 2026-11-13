# Kadı Atlası — Kullanım ve Bakım Kılavuzu

Bu kılavuz iki okur için yazıldı:

- **Araştırma ekibi** (1–3. bölümler): siteyi nasıl kullanacağınız, ekranda
  gördüğünüz sayıların ne anlama geldiği ve örnek kullanım senaryoları.
- **Bakımı yapacak kişi** (4–7. bölümler): yeni veri ekleme, koordinat
  düzeltme, yönetici parolası, barındırma (Render + Neon), yedekleme ve sorun
  giderme. Teknik ayrıntılar için ayrıca `README.md` ve `architecture.md`.

---

## 1. Temel kavramlar

| Kavram | Anlamı |
| --- | --- |
| **Kayıt** | Tablodaki bir satır: bir atama kaydı (eski kadı, yeni kadı, eski yer, yeni yer, tarih…). Aynı kayıt iki belgede tekrar yazılmışsa (aynı metin) yalnızca bir kez sayılır. |
| **Kadı** | Aynı şekilde yazılan adların *geçici* grubudur, kişi tespiti değildir. "Mehmed" adı altında yüzlerce farklı kişi toplanır; "Mehmed Mansuri" ile "Mehmed Mansurive" ise iki ayrı kadı olarak görünür. |
| **Yer** | Aynı şekilde yazılan yer adlarının grubudur. "Larende" ile "Larende’den" farklı yer sayılır (bkz. ön ek araması). |
| **Koordinat** | Yalnızca doğrulanmış koordinatı olan yerler haritaya çizilebilir. İlk teslimatta 4.678 yer adının 174'ünün koordinatı var; 23.663 kaydın yalnızca 598'inde iki yer de koordinatlı. Harita her zaman seçimin ne kadarını çizebildiğini başlığın altında yazar. |
| **Hareket yönü** | Bir kayıt, *yeni kadının* eski yerden yeni yere geçtiği ve orada eski kadının yerine atandığı şeklinde okunur. "Bu yere gelen" = yer, kaydın *yeni yeri*; "Bu yerden ayrılan" = yer, kaydın *eski yeri*. |
| **Durak** | Bir kadının güzergâhındaki her yer bir numara alır (1, 2, 3…). Haritadaki rozetler ve sağ paneldeki liste **aynı numaraları** kullanır. Aynı yere birden çok kez gelinmişse rozet hepsini gösterir: `3·21`. |
| **Tarih** | Kaynaktaki tarih ifadesi aynen gösterilir. Süzme için içindeki ilk 3–4 haneli sayı kullanılır. Miladi karşılık, yılın hicrî olduğu varsayımıyla hesaplanır ve "≈" ile işaretlenir. |

## 2. Ekranlar

**Sol panel — süzgeçler.** Burada seçtiğiniz her şey *bütün ekranlara* (harita,
tablo, grafikler, CSV) aynı şekilde uygulanır.

- *Kadı*: Bir veya birkaç kadı seçin (en fazla 8); güzergâhları haritaya çizilir
  ve Sorgu sayfasındaki tablo yalnızca bu kadıların kayıtlarını listeler.
- *Tarih aralığı* ve altındaki *Tarih gösterimi* (Hicrî / Miladi / İkisi).
- *Derece*, *Atama türü*.
- *Yer*: Bir ad yazın. Listedeki **ilk seçenek** ("“Misis” ile başlayan tüm
  yerler") o adla başlayan bütün yazımları birlikte seçer: Misis, Misis maa
  Ayaş, Misis ma‘a Ayaş-ı Perendi… Tek bir yazımı seçmek için listeden onu seçin.
- *Hareket yönü*: Bir yer seçiliyken "Bu yere gelen kadılar" veya "Bu yerden
  ayrılıp başka yere atananlar".

**Harita.** Kadı seçili değilse *Akışlar* (eski yer → yeni yer okları; kalınlık
kayıt sayısı) veya *Yerler* (daire büyüklüğü kayıt sayısı) gösterilir. Seçili
yer turuncu halkayla işaretlenir. Kadı seçiliyse güzergâh çizilir:

- düz çizgi: bir kayıt kadıyı bu duraktan bir sonrakine götürüyor;
- kesik çizgi: kayıtlarda boşluk var ya da aradaki durakların koordinatı yok —
  iz kopsa da sıradaki bilinen durağa bağlanır;
- soluk adım: süzgeçlere (yer, yön, derece, tür) uymayan adım.

Alttaki zaman çubuğunu sürükleyerek veya ▶ ile oynatarak güzergâhı/akışları
zaman içinde izleyebilirsiniz.

**Sağ panel — ayrıntı.** Seçtiğiniz kadı, yer, yer grubu, akış veya kaydın
ayrıntısı. Yer panelindeki sayılar ve kadı listesi de süzgeçlere göredir.
Bir kayda tıklayınca kaydın alanları ve (yönetici görünümünde) kaynak metin açılır.

**Sorgu.** Süzgeçlere uyan kayıtlar tablo hâlinde (sütun başlığına tıklayarak
sıralama, 50'şer sayfa) ve *Grafikler* sekmesinde: en çok atama yapılan yerler,
en çok ayrılınan yerler, zaman içinde atamalar, en çok atanan kadılar, derece
ve atama türü dağılımı. Bir çubuğa tıklamak o yeri/kadıyı açar; zaman
grafiğinde bir sütuna tıklamak tarih aralığını o döneme daraltır.
**CSV indir** süzgeçlere uyan bütün kayıtları Excel'de açılabilen bir dosya
olarak indirir (kaynak metinler hariç; yaklaşık miladi yıl sütunu dahil).

**Kaynaklar** (yalnızca yönetici görünümü). Kaynak metinlerde tam metin arama.

**Geri dönmek.** Süzgeçler, seçili kadılar, açık sayfa ve ayrıntı adres
çubuğunda saklanır. Tarayıcının **Geri** düğmesi (veya sağ panelin sol üstündeki
‹ düğmesi) bir önceki ekranı bütün süzgeçleriyle geri getirir. Adres çubuğundaki
bağlantıyı kopyalayıp gönderirseniz karşı taraf aynı ekranı görür.

## 3. Kullanım senaryoları

### Senaryo 1 — "Larende'ye atanan kadılar nereden geldi, sonra nereye gittiler?"

1. Sol paneldeki **Yer** kutusuna `Larende` yazın. Larende’den, Larendere gibi
   yazımları da kapsamak için ilk seçeneği (“Larende” ile başlayan tüm yerler),
   yalnızca "Larende" yazımı için listedeki *Larende*'yi seçin.
2. **Hareket yönü** menüsünden *Bu yere gelen (atanan) kadılar*'ı seçin.
3. Harita Larende'ye gelen okları gösterir; başlığın altındaki not kaç kaydın
   çizilebildiğini söyler. Sağ panelde Larende'ye atanan kadılar, kayıt
   sayısına göre sıralı listelenir.
4. Listeden bir kadıya tıklayın (ör. *Mehmed Emin*). Güzergâhı numaralı
   duraklarla çizilir; süzgece uyan (Larende'ye geliş) adımları vurgulu, diğer
   adımlar soluktur. Larende rozetindeki numaralar (ör. `249·290·301+2`) sağ
   paneldeki "Güzergâh" listesindeki durak numaralarıdır.
5. Kadının tüm kayıtlarını tabloda görmek için panelde **Tabloda göster**'e,
   listeye dönmek için **Geri**'ye basın.

> Not: "Mehmed Emin" gibi yaygın adlar birçok farklı kişiyi tek grupta
> toplar; böyle bir "güzergâh" birden çok kişinin kariyerinin karışımıdır.

### Senaryo 2 — "1150–1170 arasında en çok atama yapılan 10 yer"

1. Sol panelde **Tarih aralığı**: Başlangıç `1150`, Bitiş `1170`
   (*Tarih gösterimi*'ni "İkisi" yaparsanız miladi karşılık da görünür).
2. Üst menüden **Sorgu**'yu, sonra **Grafikler**'i seçin.
3. "En çok atama yapılan yerler (yeni yer)" grafiği ilk 10 yeri gösterir
   (sağ üstten 20 veya 50'ye çıkarılabilir). Zaman grafiği bu aralığı yıl yıl
   gösterir.
4. Aynı kayıtları liste olarak görmek için **Tablo**'ya geçin; Excel'de
   çalışmak için **CSV indir**.

### Senaryo 3 — "Bir kadının izini sürmek" (ör. Mehmed Mansuri)

1. **Kadı** kutusuna `Mansuri` yazıp *Mehmed Mansuri*'yi seçin.
2. Sağ paneldeki **Güzergâh** sekmesi 5 durağı tarih sırasıyla listeler;
   koordinatı olmayan duraklar "haritada yok" diye işaretlidir. Harita yalnızca
   Kelkit'i (5. durak) çizebilir; başlığın altındaki not bunu belirtir.
3. Bir durağa tıklayın: kaydın alanları ve kaynak metin açılır.
4. Koordinatlar eklendikçe aynı güzergâh kendiliğinden daha eksiksiz çizilir.

### Senaryo 4 — "Bir yerden ayrılan kadılar nereye atandı?"

1. **Yer**'de yeri seçin, **Hareket yönü**: *Bu yerden ayrılıp başka yere
   atananlar*.
2. Harita **Yerler** görünümünde gidilen yerleri, **Sorgu → Grafikler**'deki
   "En çok atama yapılan yerler" grafiği en sık gidilen yerleri gösterir.

## 4. Yeni veri ekleme (bakım)

Veri yalnızca `import_xlsx` komut satırı aracıyla yüklenir; web sitesinde veri
değiştirme ekranı yoktur. İçe aktarma:

- **tek işlemdir**: hata olursa hiçbir şey yazılmaz;
- **`doc_id`'ye göre tekrarlanabilir**: aynı dosyayı tekrar yüklemek kayıt
  çoğaltmaz, satırları günceller;
- her satırı ham hâliyle de saklar; aynı metnin ikinci kopyasını sayımlardan çıkarır.

### 4.1 Teslim edilen dosyanın biçimi

İlk teslimattaki `onaylı_data.xlsx` ile aynı: atama sayfası başlıkları
`doc_id, date, varak_no, certificate, degree, position type, period, salary,
old_salary, asitane, infisal, old_kadi, new_kadi, old_place, new_place, text`;
koordinatlar ayrı bir sayfada (`original_name, matched_name, wikidata_id,
wikipedia_url, country, official_website, lat, <boylam>`). Yeni teslimat bu
düzende olduğu sürece kod değişikliği gerekmez.

### 4.2 Canlı veritabanına (Neon) yükleme

Gereken: projenin bir kopyası ve Rust (README §1) **veya** Docker.

```bash
# 1) Neon panelinden bağlantı adresini kopyalayın (Connection pooling KAPALI).
export DATABASE_URL='postgresql://…neon.tech/neondb?sslmode=require'

# 2) Önce deneme: dosyayı okur, doğrular, hiçbir şey yazmaz.
cargo run --release --bin import_xlsx -- data/yeni_teslimat.xlsx --dry-run

# 3) Gerçek yükleme.
cargo run --release --bin import_xlsx -- data/yeni_teslimat.xlsx
```

Docker ile (Rust kurmadan):

```bash
docker build -t kadi-atlas .
docker run --rm -e DATABASE_URL="$DATABASE_URL" -v "$PWD/data:/data" \
  kadi-atlas /app/bin/import_xlsx /data/yeni_teslimat.xlsx
```

Komut sonunda bir özet yazar: kaç satır eklendi/güncellendi, kaç yinelenen
transkripsiyon bulundu, koordinat tablosunda kaç satırın kabul edildiği/
reddedildiği ve satır bazında uyarılar. Site yeni veriyi **hemen** gösterir;
yeniden dağıtım gerekmez.

Aynı dosyayı tekrar yüklediyseniz veritabanında boş alan kalır (ilk teslimatta
109 MB'lık veri yeniden yüklemelerden sonra 203 MB yer kaplıyordu). Neon'un SQL
Editor'ünde bir kez `VACUUM FULL;` çalıştırmak bu alanı geri verir.

### 4.3 Koordinat düzeltme

Koordinatlar her yüklemede doğrulanır (çalışma bölgesi dışındaki, ülke/il
merkezi olan veya tam sayı hassasiyetli noktalar haritaya alınmaz). Bir yerin
koordinatını düzeltmek veya eklemek için koordinat sayfasında o yer adının
satırını düzeltip dosyayı yeniden yükleyin; yeni karar eskisinin yerine geçer.
Bütün satırların kararı `GET /api/v1/gazetteer` adresinden görülebilir
(`?status=rejected` reddedilenler).

## 5. Yönetici görünümü / kullanıcı görünümü

- Render → servis → **Environment** → `KADI_ATLAS__ADMIN_PASSWORD` değişkenine
  en az 8 karakterlik bir parola girin ve kaydedin (servis yeniden başlar).
- Bundan sonra ziyaretçiler **kullanıcı görünümünü** görür: Kaynaklar sekmesi ve
  kaynak metinler gizlidir; sunucu da bu verileri parolasız vermez.
- Yönetici `https://<site>/admin/login` adresini açar, tarayıcının sorduğu
  parolayı girer (kullanıcı adı önemsiz) ve her şeyi görür. Çıkış:
  `/admin/logout`. Parolayı değiştirmek herkesin oturumunu kapatır.
- Değişken boşsa herkes yönetici görünümünü görür. Yönetici, sağ üstteki
  *Yönetici görünümü / Kullanıcı görünümü* düğmesiyle ziyaretçinin göreceği
  ekranı önizleyebilir.

## 6. Barındırma (Render + Neon)

| | Ücretsiz plan | Bizim kullanımımız |
| --- | --- | --- |
| Neon veritabanı | proje başına 1 GB depolama, ayda 100 CU-saat işlem | 25.826 kayıt ≈ 109 MB (kayıt başına ≈ 3,6 KB). 42 bin kayıt ≈ 170 MB, 70 bin ≈ 270 MB. |
| Render web servisi | 512 MB RAM, 0,1 CPU, ayda 750 saat; 15 dk trafik olmazsa uyur, ilk istek ~1 dk sürer | Sunucu veri tutmaz; kayıt sayısı RAM'i etkilemez. |

- Sunumdan önce siteyi bir kez açın (uykudan uyanması bir dakika sürebilir).
- Kod değişiklikleri GitHub'daki `main` dalına gönderildiğinde Render otomatik
  olarak yeniden kurar; veritabanı yapısındaki değişiklikler (`migrations/`)
  açılışta kendiliğinden uygulanır.
- **Yedek**: Neon panelinden anlık yedek (branch) alınabilir veya
  `pg_dump "$DATABASE_URL" > yedek.sql`. Asıl kaynak teslim edilen Excel
  dosyalarıdır; bunları saklamak veritabanını her zaman yeniden kurmaya yeter.

## 7. Sorun giderme

| Belirti | Olası neden ve çözüm |
| --- | --- |
| Site açılmıyor / "Veriye erişilemiyor" | Render uykudan uyanıyor (1 dk bekleyin); sürmesi hâlinde Render → Logs. Neon projesi askıya alınmış olabilir (Neon panelinde *Resume*). |
| Harita boş, "kayıt var ama hiçbiri çizilemiyor" | Seçilen kayıtların yerlerinin koordinatı yok. Kayıtlar Sorgu → Tablo'da görülebilir; koordinat eklemek için §4.3. |
| Bir kadının güzergâhı kopuk | Kayıtlarda eski yer boş veya ara yerlerin koordinatı yok; kesik çizgiler ve paneldeki "Kayıt boşluğu" satırları bunu gösterir. |
| Aynı kişi iki kez görünüyor / farklı kişiler tek adda | Ad gruplaması geçicidir (§1). Yazım birliği kaynak tabloda düzeltilip yeniden yüklenebilir. |
| İçe aktarma "hard failure" ile bitti | Hiçbir şey yazılmadı. Mesajdaki satırı Excel'de düzeltip tekrar deneyin; önce `--dry-run`. |
| Kaynaklar sekmesi görünmüyor | Kullanıcı görünümündesiniz; `/admin/login` ile giriş yapın veya sağ üstteki görünüm düğmesine bakın. |

Geliştirici notları: testler `cargo test` (README §6), kod düzeni ve veri modeli
`architecture.md`'de. Tarih dönüşümü `src/calendar.rs` ve `web/js/calendar.js`
dosyalarında aynı yöntemle yapılır; biri değişirse diğeri de güncellenmelidir.
