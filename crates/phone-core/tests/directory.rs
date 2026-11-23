use phone_core::{
    DialNumber,
    directory::{
        CallerLabel, CallerLabelText, DirectoryMatch, LabelRefusal, NationalNumber, RowRefusal,
        caller_label, directory_phone_key, parse_directory_row,
    },
};

fn national(text: &str) -> Option<NationalNumber> {
    NationalNumber::from_dial(DialNumber::parse(text.as_bytes()).ok()?)
}

#[test]
fn domestic_and_international_spellings_share_one_complete_national_number() {
    for (domestic, international) in [
        ("0312345678", "+81312345678"),
        ("09012345678", "+819012345678"),
        ("0120123456", "+81120123456"),
        ("01325400069", "+811325400069"),
    ] {
        let number = national(domestic).unwrap();
        assert_eq!(national(international), Some(number));
        assert_eq!(number.to_string(), domestic);
        assert_eq!(NationalNumber::from_key(number.key()), Some(number));
    }
    for refused in [
        "110",
        "031234567",
        "031234567890",
        "0012345678",
        "+810312345678",
        "+8131234567",
        "+12025550123",
        "312345678",
        "+81",
        "0",
    ] {
        assert_eq!(national(refused), None, "{refused}");
    }
    assert_eq!(NationalNumber::from_key(99_999_999), None);
    assert_eq!(NationalNumber::from_key(10_000_000_000), None);
}

#[test]
fn archive_telephones_use_the_national_number_or_the_digits_of_the_spelling() {
    let number = national("0112068250").unwrap();
    assert_eq!(directory_phone_key("011-206-8250"), Some(number));
    assert_eq!(directory_phone_key("(011) 206-8250"), Some(number));
    assert_eq!(
        directory_phone_key("0112068250\u{1e}０１１－２０６－８２５０"),
        Some(number)
    );
    for refused in [
        "",
        "206-8250",
        "011-206-825",
        "0112068250\u{1e}",
        "011-206-8250\u{1e}spelling",
        "\u{1e}011-206-8250",
        "0112068250\u{1e}a\u{1e}b",
    ] {
        assert_eq!(directory_phone_key(refused), None, "{refused:?}");
    }
}

#[test]
fn directory_rows_require_nine_fields_a_category_a_name_and_telephones() {
    let row = parse_directory_row(
        "3\t01101\t札幌市役所\t札幌市中央区\t011-211-2111\u{1f}0112112112\t\t7\t1\u{1f}2\t",
    )
    .unwrap();
    assert_eq!(row.category, 3);
    assert_eq!(row.name, "札幌市役所");
    assert_eq!(
        row.telephones().collect::<Vec<_>>(),
        vec![national("0112112111"), national("0112112112")]
    );
    for (line, refusal) in [
        (
            "3\t01101\t名前\t住所\t011-211-2111\t\t7\t1",
            RowRefusal::FieldCount,
        ),
        (
            "3\t01101\t名前\t住所\t011-211-2111\t\t7\t1\t\t",
            RowRefusal::FieldCount,
        ),
        (
            "03\t01101\t名前\t住所\t011-211-2111\t\t7\t1\t",
            RowRefusal::Category,
        ),
        (
            "x\t01101\t名前\t住所\t011-211-2111\t\t7\t1\t",
            RowRefusal::Category,
        ),
        (
            "\t01101\t名前\t住所\t011-211-2111\t\t7\t1\t",
            RowRefusal::Category,
        ),
        ("3\t01101\t\t住所\t011-211-2111\t\t7\t1\t", RowRefusal::Name),
        ("3\t01101\t名前\t住所\t\t\t7\t1\t", RowRefusal::Telephones),
    ] {
        assert_eq!(parse_directory_row(line).err(), Some(refusal), "{line:?}");
    }
}

#[test]
fn one_distinct_name_shows_its_sorted_categories_and_several_names_show_their_count() {
    let single = [DirectoryMatch {
        name: "栗東市 教育部学校教育課",
        category: "市区町村の役所",
    }];
    assert_eq!(
        caller_label(&single).unwrap().to_string(),
        "栗東市 教育部学校教育課（市区町村の役所）"
    );
    let repeated = [
        DirectoryMatch {
            name: "さくら病院",
            category: "病院・診療所",
        },
        DirectoryMatch {
            name: "さくら病院",
            category: "介護サービス",
        },
        DirectoryMatch {
            name: "さくら病院",
            category: "病院・診療所",
        },
    ];
    assert!(matches!(
        caller_label(&repeated),
        Some(CallerLabel::Named {
            name: "さくら病院",
            ..
        })
    ));
    assert_eq!(
        caller_label(&repeated).unwrap().to_string(),
        "さくら病院（介護サービス・病院・診療所）"
    );
    let shared = [
        DirectoryMatch {
            name: "第一小学校",
            category: "学校",
        },
        DirectoryMatch {
            name: "第二小学校",
            category: "学校",
        },
        DirectoryMatch {
            name: "第一小学校",
            category: "学校",
        },
    ];
    assert_eq!(
        caller_label(&shared).unwrap().to_string(),
        "公共施設（名前が2つあります）"
    );
    assert!(caller_label(&[]).is_none());
}

#[test]
fn label_text_is_bounded_visible_and_nonempty() {
    assert_eq!(
        CallerLabelText::new("さくら病院（病院・診療所）")
            .unwrap()
            .as_str(),
        "さくら病院（病院・診療所）"
    );
    assert!(CallerLabelText::new(&"あ".repeat(170)).is_ok());
    assert_eq!(
        CallerLabelText::new(&"あ".repeat(171)).err(),
        Some(LabelRefusal::Length)
    );
    assert_eq!(CallerLabelText::new("").err(), Some(LabelRefusal::Length));
    for refused in ["病院\n", "病院\u{1b}[31m", "病院\u{7f}", "\t病院"] {
        assert_eq!(
            CallerLabelText::new(refused).err(),
            Some(LabelRefusal::Control),
            "{refused:?}"
        );
    }
}
