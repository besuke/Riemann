// Golden values for every QuantLib C++ calendar and market, written by QuantLib itself.
//
// Build against the v1.42 sources; see tools/golden/p1_all_calendars.sh. Only the source files
// the calendars need are compiled, unchanged. Writes to the directory given as the argument:
//   calendar_markets.csv            key, region, class, market, QuantLib name, weekend days
//   calendar_all_<region>.csv       key, serial, kind for each date that is not what the weekend
//                                   days alone would give: H = holiday on a non-weekend day,
//                                   W = business day on a weekend day
//   calendar_all_errors.csv         key, first serial, last serial, message of each range of
//                                   dates where isBusinessDay throws
// Dates are serial numbers 367 (1901-01-01) to 109574 (2199-12-31); no market data is involved.

#include <ql/time/calendars/argentina.hpp>
#include <ql/time/calendars/australia.hpp>
#include <ql/time/calendars/austria.hpp>
#include <ql/time/calendars/bespokecalendar.hpp>
#include <ql/time/calendars/botswana.hpp>
#include <ql/time/calendars/brazil.hpp>
#include <ql/time/calendars/canada.hpp>
#include <ql/time/calendars/chile.hpp>
#include <ql/time/calendars/china.hpp>
#include <ql/time/calendars/czechrepublic.hpp>
#include <ql/time/calendars/denmark.hpp>
#include <ql/time/calendars/finland.hpp>
#include <ql/time/calendars/france.hpp>
#include <ql/time/calendars/germany.hpp>
#include <ql/time/calendars/hongkong.hpp>
#include <ql/time/calendars/hungary.hpp>
#include <ql/time/calendars/iceland.hpp>
#include <ql/time/calendars/india.hpp>
#include <ql/time/calendars/indonesia.hpp>
#include <ql/time/calendars/israel.hpp>
#include <ql/time/calendars/italy.hpp>
#include <ql/time/calendars/japan.hpp>
#include <ql/time/calendars/jointcalendar.hpp>
#include <ql/time/calendars/mexico.hpp>
#include <ql/time/calendars/newzealand.hpp>
#include <ql/time/calendars/norway.hpp>
#include <ql/time/calendars/nullcalendar.hpp>
#include <ql/time/calendars/poland.hpp>
#include <ql/time/calendars/romania.hpp>
#include <ql/time/calendars/russia.hpp>
#include <ql/time/calendars/saudiarabia.hpp>
#include <ql/time/calendars/singapore.hpp>
#include <ql/time/calendars/slovakia.hpp>
#include <ql/time/calendars/southafrica.hpp>
#include <ql/time/calendars/southkorea.hpp>
#include <ql/time/calendars/sweden.hpp>
#include <ql/time/calendars/switzerland.hpp>
#include <ql/time/calendars/taiwan.hpp>
#include <ql/time/calendars/target.hpp>
#include <ql/time/calendars/thailand.hpp>
#include <ql/time/calendars/turkey.hpp>
#include <ql/time/calendars/ukraine.hpp>
#include <ql/time/calendars/unitedkingdom.hpp>
#include <ql/time/calendars/unitedstates.hpp>
#include <ql/time/calendars/weekendsonly.hpp>
#include <ql/version.hpp>

#include <algorithm>
#include <exception>
#include <fstream>
#include <iostream>
#include <map>
#include <string>
#include <vector>

using namespace QuantLib;

namespace {

    struct Entry {
        std::string key, region, cls, market;
        Calendar calendar;
    };

    std::vector<Entry> entries() {
        BespokeCalendar bespoke("Riemann test");
        bespoke.addWeekend(Wednesday);
        bespoke.addWeekend(Thursday);
        Calendar jp = Japan(), xetra = Germany(Germany::Xetra), bovespa = Brazil(Brazil::Exchange),
                 moex = Russia(Russia::MOEX);
        return {
            {"null_calendar", "common", "NullCalendar", "", NullCalendar()},
            {"weekends_only", "common", "WeekendsOnly", "", WeekendsOnly()},
            {"bespoke_wednesday_thursday", "common", "BespokeCalendar", "", bespoke},
            {"joint_three_holidays", "common", "JointCalendar", "JoinHolidays", JointCalendar(jp, xetra, bovespa, JoinHolidays)},
            {"joint_three_business_days", "common", "JointCalendar", "JoinBusinessDays", JointCalendar(jp, xetra, bovespa, JoinBusinessDays)},
            {"joint_japan_moex_holidays", "common", "JointCalendar", "JoinHolidays", JointCalendar(jp, moex, JoinHolidays)},
            {"joint_japan_moex_business_days", "common", "JointCalendar", "JoinBusinessDays", JointCalendar(jp, moex, JoinBusinessDays)},

            {"argentina_merval", "americas", "Argentina", "Merval", Argentina(Argentina::Merval)},
            {"brazil_settlement", "americas", "Brazil", "Settlement", Brazil(Brazil::Settlement)},
            {"brazil_exchange", "americas", "Brazil", "Exchange", Brazil(Brazil::Exchange)},
            {"canada_settlement", "americas", "Canada", "Settlement", Canada(Canada::Settlement)},
            {"canada_tsx", "americas", "Canada", "TSX", Canada(Canada::TSX)},
            {"chile_sse", "americas", "Chile", "SSE", Chile(Chile::SSE)},
            {"mexico_bmv", "americas", "Mexico", "BMV", Mexico(Mexico::BMV)},
            {"us_settlement", "americas", "UnitedStates", "Settlement", UnitedStates(UnitedStates::Settlement)},
            {"us_nyse", "americas", "UnitedStates", "NYSE", UnitedStates(UnitedStates::NYSE)},
            {"us_government_bond", "americas", "UnitedStates", "GovernmentBond", UnitedStates(UnitedStates::GovernmentBond)},
            {"us_nerc", "americas", "UnitedStates", "NERC", UnitedStates(UnitedStates::NERC)},
            {"us_libor_impact", "americas", "UnitedStates", "LiborImpact", UnitedStates(UnitedStates::LiborImpact)},
            {"us_federal_reserve", "americas", "UnitedStates", "FederalReserve", UnitedStates(UnitedStates::FederalReserve)},
            {"us_sofr", "americas", "UnitedStates", "SOFR", UnitedStates(UnitedStates::SOFR)},

            {"target", "europe", "TARGET", "", TARGET()},
            {"austria_settlement", "europe", "Austria", "Settlement", Austria(Austria::Settlement)},
            {"austria_exchange", "europe", "Austria", "Exchange", Austria(Austria::Exchange)},
            {"czech_republic_pse", "europe", "CzechRepublic", "PSE", CzechRepublic(CzechRepublic::PSE)},
            {"denmark", "europe", "Denmark", "", Denmark()},
            {"finland", "europe", "Finland", "", Finland()},
            {"france_settlement", "europe", "France", "Settlement", France(France::Settlement)},
            {"france_exchange", "europe", "France", "Exchange", France(France::Exchange)},
            {"germany_settlement", "europe", "Germany", "Settlement", Germany(Germany::Settlement)},
            {"germany_frankfurt_stock_exchange", "europe", "Germany", "FrankfurtStockExchange", Germany(Germany::FrankfurtStockExchange)},
            {"germany_xetra", "europe", "Germany", "Xetra", Germany(Germany::Xetra)},
            {"germany_eurex", "europe", "Germany", "Eurex", Germany(Germany::Eurex)},
            {"germany_euwax", "europe", "Germany", "Euwax", Germany(Germany::Euwax)},
            {"hungary", "europe", "Hungary", "", Hungary()},
            {"iceland_icex", "europe", "Iceland", "ICEX", Iceland(Iceland::ICEX)},
            {"italy_settlement", "europe", "Italy", "Settlement", Italy(Italy::Settlement)},
            {"italy_exchange", "europe", "Italy", "Exchange", Italy(Italy::Exchange)},
            {"norway", "europe", "Norway", "", Norway()},
            {"poland_settlement", "europe", "Poland", "Settlement", Poland(Poland::Settlement)},
            {"poland_wse", "europe", "Poland", "WSE", Poland(Poland::WSE)},
            {"romania_public", "europe", "Romania", "Public", Romania(Romania::Public)},
            {"romania_bvb", "europe", "Romania", "BVB", Romania(Romania::BVB)},
            {"russia_settlement", "europe", "Russia", "Settlement", Russia(Russia::Settlement)},
            {"russia_moex", "europe", "Russia", "MOEX", Russia(Russia::MOEX)},
            {"slovakia_bsse", "europe", "Slovakia", "BSSE", Slovakia(Slovakia::BSSE)},
            {"sweden", "europe", "Sweden", "", Sweden()},
            {"switzerland", "europe", "Switzerland", "", Switzerland()},
            {"ukraine_use", "europe", "Ukraine", "USE", Ukraine(Ukraine::USE)},
            {"uk_settlement", "europe", "UnitedKingdom", "Settlement", UnitedKingdom(UnitedKingdom::Settlement)},
            {"uk_exchange", "europe", "UnitedKingdom", "Exchange", UnitedKingdom(UnitedKingdom::Exchange)},
            {"uk_metals", "europe", "UnitedKingdom", "Metals", UnitedKingdom(UnitedKingdom::Metals)},

            {"botswana", "middle_east_africa", "Botswana", "", Botswana()},
            {"israel_settlement", "middle_east_africa", "Israel", "Settlement", Israel(Israel::Settlement)},
            {"israel_tase", "middle_east_africa", "Israel", "TASE", Israel(Israel::TASE)},
            {"israel_shir", "middle_east_africa", "Israel", "SHIR", Israel(Israel::SHIR)},
            {"saudi_arabia_tadawul", "middle_east_africa", "SaudiArabia", "Tadawul", SaudiArabia(SaudiArabia::Tadawul)},
            {"south_africa", "middle_east_africa", "SouthAfrica", "", SouthAfrica()},
            {"turkey", "middle_east_africa", "Turkey", "", Turkey()},

            {"japan", "asia_pacific", "Japan", "", Japan()},
            {"australia_settlement", "asia_pacific", "Australia", "Settlement", Australia(Australia::Settlement)},
            {"australia_asx", "asia_pacific", "Australia", "ASX", Australia(Australia::ASX)},
            {"china_sse", "asia_pacific", "China", "SSE", China(China::SSE)},
            {"china_ib", "asia_pacific", "China", "IB", China(China::IB)},
            {"hong_kong_hkex", "asia_pacific", "HongKong", "HKEx", HongKong(HongKong::HKEx)},
            {"india_nse", "asia_pacific", "India", "NSE", India(India::NSE)},
            {"indonesia_bej", "asia_pacific", "Indonesia", "BEJ", Indonesia(Indonesia::BEJ)},
            {"indonesia_jsx", "asia_pacific", "Indonesia", "JSX", Indonesia(Indonesia::JSX)},
            {"indonesia_idx", "asia_pacific", "Indonesia", "IDX", Indonesia(Indonesia::IDX)},
            {"new_zealand_wellington", "asia_pacific", "NewZealand", "Wellington", NewZealand(NewZealand::Wellington)},
            {"new_zealand_auckland", "asia_pacific", "NewZealand", "Auckland", NewZealand(NewZealand::Auckland)},
            {"singapore_sgx", "asia_pacific", "Singapore", "SGX", Singapore(Singapore::SGX)},
            {"south_korea_settlement", "asia_pacific", "SouthKorea", "Settlement", SouthKorea(SouthKorea::Settlement)},
            {"south_korea_krx", "asia_pacific", "SouthKorea", "KRX", SouthKorea(SouthKorea::KRX)},
            {"taiwan_tsec", "asia_pacific", "Taiwan", "TSEC", Taiwan(Taiwan::TSEC)},
            {"thailand", "asia_pacific", "Thailand", "", Thailand()},
        };
    }

    std::string cleaned(const std::string& message) {
        // the location prefix depends on the build; keep the text after the last ": " or newline
        std::string text = message;
        auto at = text.find_last_of('\n');
        if (at != std::string::npos)
            text = text.substr(at + 1);
        std::replace(text.begin(), text.end(), ',', ';');
        // numbers vary by date; the message is compared by cause
        std::string result;
        for (char c : text) {
            if (c >= '0' && c <= '9') {
                if (result.empty() || result.back() != 'N')
                    result += 'N';
            } else {
                result += c;
            }
        }
        return result;
    }

}

int main(int argc, char** argv) {
    if (argc != 2) {
        std::cerr << "usage: p1_all_calendars <output directory>\n";
        return 1;
    }
    const std::string out = std::string(argv[1]) + "/";
    const std::string source = std::string("QuantLib C++ ") + QL_VERSION + " (v1.42 tag commit 41b0e14), built from source";
    const std::vector<Entry> all = entries();

    std::map<std::string, std::ofstream> files;
    for (const auto& e : all) {
        if (files.count(e.region) == 0) {
            auto& f = files[e.region];
            f.open(out + "calendar_all_" + e.region + ".csv", std::ios::binary);
            f << "# " << source << "; H = holiday on a non-weekend day, W = business day on a weekend day\n";
            f << "calendar,serial,kind\n";
        }
    }
    std::ofstream markets(out + "calendar_markets.csv", std::ios::binary);
    markets << "# " << source << "; weekend days are numbers with Sunday = 1\n";
    markets << "key,region,class,market,name,weekend\n";
    std::ofstream errors(out + "calendar_all_errors.csv", std::ios::binary);
    errors << "# " << source << "; ranges of dates where isBusinessDay throws, including the first and last\n";
    errors << "calendar,first,last,message\n";

    const Weekday days[] = {Sunday, Monday, Tuesday, Wednesday, Thursday, Friday, Saturday};
    for (const auto& e : all) {
        bool weekend[8] = {false};
        std::string weekend_text;
        for (Weekday w : days) {
            weekend[static_cast<int>(w)] = e.calendar.isWeekend(w);
            if (weekend[static_cast<int>(w)])
                weekend_text += (weekend_text.empty() ? "" : " ") + std::to_string(static_cast<int>(w));
        }
        std::string name = e.calendar.name();
        std::replace(name.begin(), name.end(), ',', ';');
        markets << e.key << ',' << e.region << ',' << e.cls << ',' << e.market << ',' << name << ',' << weekend_text << '\n';

        auto& f = files[e.region];
        std::size_t holidays = 0, weekend_business = 0, error_days = 0;
        Date::serial_type range_first = 0, range_last = 0;
        std::string range_message;
        auto flush = [&]() {
            if (range_first != 0)
                errors << e.key << ',' << range_first << ',' << range_last << ',' << range_message << '\n';
            range_first = 0;
        };
        for (Date::serial_type s = 367; s <= 109574; ++s) {
            const Date d(s);
            try {
                const bool business = e.calendar.isBusinessDay(d);
                flush();
                const bool on_weekend = weekend[static_cast<int>(d.weekday())];
                if (!business && !on_weekend) {
                    f << e.key << ',' << s << ",H\n";
                    ++holidays;
                } else if (business && on_weekend) {
                    f << e.key << ',' << s << ",W\n";
                    ++weekend_business;
                }
            } catch (const std::exception& x) {
                const std::string message = cleaned(x.what());
                if (range_first != 0 && range_last == s - 1 && range_message == message) {
                    range_last = s;
                } else {
                    flush();
                    range_first = range_last = s;
                    range_message = message;
                }
                ++error_days;
            }
        }
        flush();
        std::cout << e.key << ": H " << holidays << ", W " << weekend_business << ", errors " << error_days << "\n";
    }
    return 0;
}
