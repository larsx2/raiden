import { describe, expect, it } from "vitest";
import { convert } from "./convert";

const RATES = { USD: 1, EUR: 0.9, MXN: 18, GBP: 0.8 };

describe("convert", () => {
  it("converts between units of the same kind", () => {
    expect(convert("10 kg to lb")?.to).toBe("22.0462262185 lb");
    expect(convert("1 mi in km")?.to).toBe("1.609344 km");
    expect(convert("5 in in cm")?.to).toBe("12.7 cm");
    expect(convert("1,000 ft to m")?.to).toBe("304.8 m");
    expect(convert("1,5 kg to g")?.to).toBe("1500 g");
    expect(convert("100 km/h to mph")?.to).toBe("62.1371192237 mph");
    expect(convert("1 atm to psi")?.to).toBe("14.6959487755 psi");
    expect(convert("2 acres to m2")?.to).toBe("8093.7128448 m²");
    expect(convert("1 GiB to MB")?.to).toBe("1073.741824 MB");
    expect(convert("3 nautical miles to km")?.to).toBe("5.556 km");
  });

  it("reads comma decimals and thousands apart, and tells symbols apart by case", () => {
    expect(convert("0,125 kg to g")?.to).toBe("125 g");
    expect(convert("12,500 g to kg")?.to).toBe("12.5 kg");
    expect(convert("1 sq in in cm2")?.to).toBe("6.4516 cm²");
    expect(convert("500 mb to gb")?.to).toBe("0.5 GB");
    expect(convert("500 mL to cups")?.to).toBe("2.11337641887 cup");
    expect(convert("1 Celsius to Fahrenheit")?.to).toBe("33.8 °F");
    expect(convert("10 Kg to lb")?.to).toBe("22.0462262185 lb");
    expect(convert("1 Mile to Km")?.to).toBe("1.609344 km");
    expect(convert("1 KWh to J")?.to).toBe("3600000 J");
    expect(convert(`1 kg${" to".repeat(100)} lb`)).toBeUndefined();
    for (const text of ["1 Gb to MB", "1 mPa to Pa", "1 Mg to kg", "1 nm to m"]) {
      expect(convert(text)).toBeUndefined();
    }
  });

  it("shows every digit of whole answers and no exponents for tiny ones", () => {
    expect(convert("1 TiB to B")?.to).toBe("1099511627776 B");
    expect(convert("1 mg to t")?.to).toBe("0.000000001 t");
  });

  it("handles the offsets of temperatures", () => {
    expect(convert("100 c to f")?.to).toBe("212 °F");
    expect(convert("-40°F to °C")?.to).toBe("-40 °C");
    expect(convert("0 K to celsius")?.to).toBe("-273.15 °C");
  });

  it("converts currencies with the rates it is given", () => {
    expect(convert("100 usd to mxn", RATES)).toEqual({
      from: "100.00 USD",
      to: "1,800.00 MXN",
      value: "1800.00",
    });
    expect(convert("$90 to eur", RATES)?.to).toBe("81.00 EUR");
    expect(convert("1 usd to gbp", RATES)?.to).toBe("0.80 GBP");
    expect(convert("10 euros in gbp", RATES)?.to).toBe("8.89 GBP");
    expect(convert("100 usd to mxn")).toBeUndefined();
    expect(convert("1 usd to xbt", { USD: 1, XBT: 0.00001234 })?.to).toBe("0.00001234 XBT");
    expect(convert("€90 usd to mxn", RATES)).toBeUndefined();
  });

  it("stays quiet for anything it cannot convert", () => {
    for (const text of [
      "",
      "kg to lb",
      "10 kg",
      "10 kg to km",
      "10 kg to kg",
      "10 foo to bar",
      "100 to eur",
      "how to cook rice",
    ]) {
      expect(convert(text, RATES)).toBeUndefined();
    }
  });
});
