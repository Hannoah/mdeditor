// SAS 코드 블록 하이라이트.
// 키워드 목록 출처: KDE syntax-highlighting data/syntax/sas.xml
//   SPDX-FileCopyrightText: 2026 Michael Walshe — SPDX-License-Identifier: MIT
use std::collections::HashMap;
use std::sync::OnceLock;

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

// 문(statement) 키워드: controls + keywords + datastep-keywords (+ sas.xml이 문맥 규칙으로 처리하는 DATA/PROC/RUN/QUIT 등)
const STATEMENTS: &str = "DATA PROC RUN QUIT OPTIONS OPTION \
BY DO ELSE END GOTO IF LINK OTHERWISE SELECT THEN TO UNTIL WHEN WHILE \
AXIS CAS CASLIB CATNAME DECLARE ENDRSUBMIT ENDSAS FILENAME FOOTNOTE GOPTIONS KILLTASK LEGEND LIBNAME \
LISTTASK LOCK MISSING ODS PAGE PATTERN PROCEDURE RESETLINE RGET RSPT RSUBMIT SASFILE SIGNOFF SIGNON \
SKIP SYMBOL SYSECHO SYSTASK TITLE TRANTAB WAITFOR \
ABORT ARRAY ATTRIB CALL CONTINUE DELETE DESCRIBE DROP ERROR EXECUTE FILE FORMAT INFILE INFORMAT INPUT \
KEEP LABEL LEAVE LENGTH LIST LOSTCARD MERGE MODIFY OUTPUT PUT PUTLOG REDIRECT REMOVE RENAME REPLACE \
RETAIN RETURN SET STOP UPDATE WHERE \
CREATE TABLE AS FROM GROUP ORDER HAVING JOIN LEFT RIGHT INNER OUTER ON CASE DISTINCT INTO VALUES";

const OPERATORS: &str =
    "AND BETWEEN CONTAINS EQ EQT GE GET GT IN IS LE LET LIKE LT NE NET NOT OR SAME ALSO";

const CONSTANTS: &str =
    "_ALL_ _BLANKPAGE_ _CHARACTER_ _ERROR_ _FILE_ _INFILE_ _IORC_ _MRG_ _NULL_ \
_NUMERIC_ _N_ _PAGE_ _TEMPORARY_";

const FUNCTIONS: &str = "ABS ADDMATRIX ADDR ADDRLONG ADDROW ADD_TABLE_ATTR AIRY ALLCOMB ALLCOMBI ALLPERM \
ANYALNUM ANYALPHA ANYCNTRL ANYDIGIT ANYFIRST ANYGRAPH ANYLOWER ANYNAME ANYPRINT ANYPUNCT ANYSPACE ANYUPPER \
ANYXDIGIT ARCOS ARCOSH ARMCONV ARMEND ARMGTID ARMINIT ARMJOIN ARMPROC ARMSTOP ARMSTRT ARMUPDT ARSIN ARSINH \
ARTANH ASCEBC ATAN ATAN2 ATTRC ATTRN BAND BETA BETAINV BLACKCLPRC BLACKPTPRC BLKSHCLPRC BLKSHPTPRC BLSHIFT \
BNOT BOR BQUOTE BRSHIFT BXOR BYTE CAT CATQ CATS CATT CATX CDF CEIL CEILZ CEXIST CHAR CHOL CHOOSEC CHOOSEN \
CINV CLIBEXIST CLOSE CMISS CMPRES CNONCT COALESCE COALESCEC COLLATE COMB COMPANION_NEXT COMPARE COMPBL \
COMPCOST COMPFUZZ COMPGED COMPLEV COMPOUND COMPRESS COMPSTOR CONSTANT CONVX CONVXP COS COSH COT COUNT \
COUNTC COUNTW CSC CSS CUMIPMT CUMPRINC CUROBS CV DACCDB DACCDBSL DACCSL DACCSYD DACCTAB DAIRY DATATYP \
DATDIF DATE DATEJUL DATEPART DATETIME DAY DCLOSE DCREATE DEFINE DEPDB DEPDBSL DEPSL DEPSYD DEPTAB DEQUOTE \
DET DEVIANCE DHMS DICTIONARY DIF DIGAMMA DIM DINFO DISCARD DIVIDE DNUM DOPEN DOPTNAME DOPTNUM DOSUBL \
DQCASE DQGENDER DQGENDERINFOGET DQGENDERPARSED DQIDENTIFY DQLOCALEGUESS DQLOCALEINFOGET DQLOCALEINFOLIST \
DQMATCH DQMATCHINFOGET DQMATCHPARSED DQPARSE DQPARSEINFOGET DQPARSETOKENGET DQPARSETOKENPUT DQPATTERN \
DQSCHEMEAPPLY DQSRVARCHJOB DQSRVCOPYLOG DQSRVDELETELOG DQSRVJOBSTATUS DQSRVKILLJOB DQSRVPROFJOBFILE \
DQSRVPROFJOBREP DQSRVUSER DQSTANDARDIZE DQTOKEN DREAD DROPNOTE DSNAME DSNCATLGD DUR DURP DYNAMIC_ARRAY \
EBCASC EFFRATE ELEMMULT ENTRY_FIRST ENTRY_NEXT ENVLEN ERF ERFC EUCLID EVAL EXECUTE EXIST EXISTS EXP \
EXPMATRIX FACT FAPPEND FCLOSE FCOL FCOPY FDELETE FETCH FETCHOBS FEXIST FGET FILEATTR FILEEXIST FILENAME \
FILEREF FILLMATRIX FINANCE FIND FINDC FINDFILE FINDTABLE FINDW FINFO FINV FIPNAME FIPNAMEL FIPSTATE FIRST \
FLOOR FLOORZ FMTINFO FNONCT FNOTE FOPEN FOPTNAME FOPTNUM FPOINT FPOS FPUT FREAD FREWIND FRLEN FSEP FUZZ \
FWRITE GAMINV GAMMA GARKHCLPRC GARKHPTPRC GCD GEODIST GEOMEAN GEOMEANZ GETCASURL GETDVI GETJPI GETLCASLIB \
GETLOG GETLSESSREF GETLTAG GETMSG GETOPTION GETQUOTA GETSESSOPT GETSYM GETTERM GETVARC GETVARN GITFN_CLONE \
GITFN_COMMIT GITFN_COMMITFREE GITFN_COMMIT_GET GITFN_COMMIT_LOG GITFN_CO_BRANCH GITFN_DEL_REPO GITFN_DIFF \
GITFN_DIFF_FREE GITFN_DIFF_GET GITFN_DIFF_IDX_F GITFN_IDX_ADD GITFN_IDX_REMOVE GITFN_MRG_BRANCH \
GITFN_NEW_BRANCH GITFN_PULL GITFN_PUSH GITFN_RESET GITFN_RESET_FILE GITFN_STATUS GITFN_STATUSFREE \
GITFN_STATUS_GET GITFN_VERSION GRAYCODE GRDSVC_ENABLE GRDSVC_GETADDR GRDSVC_GETINFO GRDSVC_GETNAME \
GRDSVC_NNODES HARMEAN HARMEANZ HASHING HASHING_FILE HASHING_HMAC HASHING_HMAC_FILE HASHING_HMAC_INIT \
HASHING_INIT HASHING_PART HASHING_TERM HBOUND HMS HOLIDAY HOLIDAYCK HOLIDAYCOUNT HOLIDAYNAME HOLIDAYNX \
HOLIDAYNY HOLIDAYTEST HOUR HTMLDECODE HTMLENCODE IBESSEL IDENTITY IFC IFN INDEX INDEXC INDEXW INPUT INPUTC \
INPUTN INSERT_CATALOG INSERT_DATASET INSERT_FDB INSERT_FILE INSERT_HTML INSERT_MDDB INSERT_PACKAGE \
INSERT_REF INSERT_SQLVIEW INSERT_VIEWER INT INTCINDEX INTCK INTCYCLE INTFIT INTFMT INTGET INTINDEX INTNEST \
INTNX INTRR INTSEAS INTSHIFT INTTEST INTZ INV INVCDF INVERSE IORCMSG IPMT IQR IRR IS8601_CONVERT ISARRAY \
ISBLOB ISDICTIONARY ISDOUBLE ISINTEGER ISLIST ISNULL ISSTRING ISTABLE ISTYPE JBESSEL JSONPP JULDATE \
JULDATE7 KCOMPARE KCOMPRESS KCOUNT KINDEX KINDEXC KLEFT KLENGTH KLOWCASE KREVERSE KRIGHT KSCAN KSUBSTR \
KSUBSTRB KTRANSLATE KTRIM KTRUNCATE KUPCASE KUPDATE KUPDATEB KURTOSIS KVERIFY LABEL LAG LARGEST LBOUND LCM \
LCOMB LEFT LENGTH LENGTHC LENGTHM LENGTHN LEXCOMB LEXCOMBI LEXPERK LEXPERM LFACT LGAMMA LIBNAME LIBREF \
LIMMOMENT LOG LOG10 LOG1PX LOG2 LOGBETA LOGCDF LOGISTIC LOGPDF LOGSDF LOWCASE LPERM LPNORM MAD MARGRCLPRC \
MARGRPTPRC MAX MD5 MDY MEAN MEDIAN MIN MINUTE MISSING MOD MODEXIST MODULE MODULEC MODULEN MODZ MONTH MOPEN \
MORT MSPLINT MULT MVALID N NETPV NEWTABLE NLITERAL NMISS NODENAME NOMRATE NORMAL NOTALNUM NOTALPHA \
NOTCNTRL NOTDIGIT NOTE NOTFIRST NOTGRAPH NOTLOWER NOTNAME NOTPRINT NOTPUNCT NOTSPACE NOTUPPER NOTXDIGIT \
NPV NRBQUOTE NRQUOTE NRSTR NVALID NWKDOM OPEN ORDINAL PACKAGE_BEGIN PACKAGE_DESTROY PACKAGE_END \
PACKAGE_FIRST PACKAGE_NEXT PACKAGE_PUBLISH PACKAGE_TERM PATHNAME PCTL PDF PEEK PEEKC PEEKCLONG PEEKLONG \
PERM PMF PMT POINT POISSON POKE POKELONG POWER PPMT PRINTTABLE PROBBETA PROBBNML PROBBNRM PROBCHI PROBF \
PROBGAM PROBHYPR PROBIT PROBMC PROBMED PROBNEGB PROBNORM PROBT PROPCASE PRXCHANGE PRXDEBUG PRXFREE \
PRXMATCH PRXNEXT PRXPAREN PRXPARSE PRXPOSN PRXSUBSTR PTRLONGADD PUT PUTC PUTLOG PUTN PUTSYM PVP QCMPRES \
QLEFT QLOWCASE QSCAN QSUBSTR QSYSFUNC QTR QTRIM QUANTILE QUOTE QUPCASE RANBIN RANCAU RANCOMB RAND RANEXP \
RANGAM RANGE RANK RANNOR RANPERK RANPERM RANPOI RANTBL RANTRI RANUNI READPATH READ_ARRAY RENAME REPEAT \
RESOLVE RETRIEVE_CATALOG RETRIEVE_DATASET RETRIEVE_FDB RETRIEVE_FILE RETRIEVE_HTML RETRIEVE_MDDB \
RETRIEVE_NESTED RETRIEVE_PACKAGE RETRIEVE_REF RETRIEVE_SQLVIEW RETRIEVE_VIEWER REVERSE REWIND RIGHT RMS \
ROUND ROUNDE ROUNDZ RUN_MACRO RUN_SASFILE RXCHANGE RXFREE RXSUBSTR SAVING SAVINGS SCAN SCANQ SDF SEC \
SECOND SESSFOUND SESSIONS SET SETNULL SETTERM SHA256 SHA256HEX SHA256HMACHEX SIGN SIN SINH SKEWNESS SLEEP \
SMALLEST SOAPWEB SOAPWEBMETA SOAPWIPSERVICE SOAPWIPSRS SOAPWS SOAPWSMETA SOFTMAX SOLVE SORT SORTC SORTN \
SORT_REV SOUNDEX SPEDIS SQRT SQUANTILE STD STDERR STDIZE STFIPS STNAME STNAMEL STR STREAMINIT STRIP \
STRUCTINDEX SUBPAD SUBSTR SUBSTRN SUBTRACTMATRIX SUM SUMABS SUPERQ SYMEXIST SYMGET SYMGLOBL SYMLOCAL \
SYMPUT SYMPUTX SYSEVALF SYSEXIST SYSFUNC SYSGET SYSMSG SYSPARM SYSPROCESSID SYSPROCESSNAME SYSPROD SYSRC \
SYSTEM TABCOLUMNS TABTYPES TAN TANH TERMIN TERMOUT TIME TIMEPART TIMEVALUE TINV TNONCT TODAY TRACEBACK \
TRANSLATE TRANSPOSE TRANSTRN TRANWRD TRIGAMMA TRIM TRIMN TRUNC TSO TTCLOSE TTCONTRL TTOPEN TTREAD TTWRITE \
TYPEOF TZID TZONEID TZONENAME TZONEOFF TZONES2U TZONEU2S UNIFORM UNQUOTE UPCASE URLDECODE URLENCODE USS \
UUIDGEN VAR VARFMT VARINFMT VARLABEL VARLEN VARNAME VARNUM VARRAY VARRAYX VARTYPE VERIFY VFORMAT VFORMATD \
VFORMATDX VFORMATN VFORMATNX VFORMATW VFORMATWX VFORMATX VINARRAY VINARRAYX VINFORMAT VINFORMATD \
VINFORMATDX VINFORMATN VINFORMATNX VINFORMATW VINFORMATWX VINFORMATX VLABEL VLABELX VLENGTH VLENGTHX VMS \
VNAME VNAMEX VNEXT VTYPE VTYPEX VVALUE VVALUEX WEEK WEEKDAY WHICHC WHICHN WRITE_ARRAY WTO YEAR YIELDP \
YRDIF YYQ ZEROMATRIX ZIPCITY ZIPCITYDISTANCE ZIPFIPS ZIPNAME ZIPNAMEL ZIPSTATE";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Plain,
    Keyword,
    Function,
    Constant,
    Str,
    Number,
    Comment,
    Macro,
}

fn words() -> &'static HashMap<&'static str, Kind> {
    static W: OnceLock<HashMap<&'static str, Kind>> = OnceLock::new();
    W.get_or_init(|| {
        let mut m = HashMap::new();
        // 함수 → 문 순서: 둘 다인 단어(SET, INPUT…)는 문 키워드가 이긴다 (함수는 "(" 앞에서만 따로 판정)
        for w in FUNCTIONS.split_whitespace() {
            m.insert(w, Kind::Function);
        }
        for w in OPERATORS
            .split_whitespace()
            .chain(STATEMENTS.split_whitespace())
        {
            m.insert(w, Kind::Keyword);
        }
        for w in CONSTANTS.split_whitespace() {
            m.insert(w, Kind::Constant);
        }
        m
    })
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// (바이트 구간, 종류) 목록. 구간 경계는 모두 ASCII 위치라 UTF-8 경계가 안전하다.
fn tokenize(code: &str) -> Vec<(usize, usize, Kind)> {
    let b = code.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let (mut i, mut stmt_start, mut after_proc) = (0, true, false);
    let until_semi = |from: usize| code[from..].find(';').map_or(n, |k| from + k + 1);
    while i < n {
        let c = b[i];
        let start = i;
        let kind = if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = code[i + 2..].find("*/").map_or(n, |k| i + 2 + k + 2);
            Kind::Comment
        } else if c == b'*' && stmt_start {
            i = until_semi(i); // * 주석 문장 ;
            Kind::Comment
        } else if c == b'%' && b.get(i + 1) == Some(&b'*') {
            i = until_semi(i);
            Kind::Comment
        } else if c == b'\'' || c == b'"' {
            i += 1;
            while i < n {
                if b[i] == c {
                    if b.get(i + 1) == Some(&c) {
                        i += 2; // '' 이스케이프
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            let close = i;
            while i < n && i - close < 2 && b[i].is_ascii_alphabetic() {
                i += 1; // 'd 'dt 't 'n 'x 'b 접미사
            }
            Kind::Str
        } else if c == b'%' && b.get(i + 1).is_some_and(|&x| x.is_ascii_alphabetic()) {
            i += 1;
            while i < n && is_word(b[i]) {
                i += 1;
            }
            Kind::Macro
        } else if c == b'&'
            && b.get(i + 1)
                .is_some_and(|&x| x == b'&' || x.is_ascii_alphabetic() || x == b'_')
        {
            while i < n && b[i] == b'&' {
                i += 1;
            }
            while i < n && is_word(b[i]) {
                i += 1;
            }
            if b.get(i) == Some(&b'.') {
                i += 1; // &name. 구분점
            }
            Kind::Macro
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                i += 1;
            }
            Kind::Number
        } else if is_word(c) {
            while i < n && is_word(b[i]) {
                i += 1;
            }
            let w = code[start..i].to_ascii_uppercase();
            let next = code[i..].trim_start().as_bytes().first().copied();
            let k = if after_proc {
                Kind::Function // PROC 다음 프로시저 이름
            } else {
                match words().get(w.as_str()) {
                    Some(Kind::Function) if next == Some(b'(') => Kind::Function,
                    Some(Kind::Function) | None => Kind::Plain,
                    Some(&k) => k,
                }
            };
            after_proc = w == "PROC";
            k
        } else {
            i += code[i..].chars().next().map_or(1, char::len_utf8);
            Kind::Plain
        };
        if !c.is_ascii_whitespace() {
            stmt_start = b[i - 1] == b';'; // 주석 문장 "* …;" 뒤도 새 문장의 시작
        }
        if kind != Kind::Plain
            || out
                .last()
                .is_none_or(|t: &(usize, usize, Kind)| t.2 != Kind::Plain)
        {
            out.push((start, i, kind));
        } else if let Some(last) = out.last_mut() {
            last.1 = i;
        }
    }
    out
}

pub fn highlight(
    code: &str,
    font: FontId,
    line_height: f32,
    dark: bool,
    text: Color32,
) -> LayoutJob {
    let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    // VS Code 기본 테마 계열 색
    let color = |k: Kind| match (k, dark) {
        (Kind::Plain, _) => text,
        (Kind::Keyword, true) => hex(0x569CD6),
        (Kind::Keyword, false) => hex(0x0000FF),
        (Kind::Function, true) => hex(0xDCDCAA),
        (Kind::Function, false) => hex(0x795E26),
        (Kind::Constant, true) => hex(0x4FC1FF),
        (Kind::Constant, false) => hex(0x0070C1),
        (Kind::Str, true) => hex(0xCE9178),
        (Kind::Str, false) => hex(0xA31515),
        (Kind::Number, true) => hex(0xB5CEA8),
        (Kind::Number, false) => hex(0x098658),
        (Kind::Comment, true) => hex(0x6A9955),
        (Kind::Comment, false) => hex(0x008000),
        (Kind::Macro, true) => hex(0xC586C0),
        (Kind::Macro, false) => hex(0xAF00DB),
    };
    let mut job = LayoutJob::default();
    for (a, b, k) in tokenize(code) {
        job.append(
            &code[a..b],
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: color(k),
                italics: k == Kind::Comment,
                line_height: Some(line_height),
                ..Default::default()
            },
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(code: &str) -> Vec<(&str, Kind)> {
        tokenize(code)
            .into_iter()
            .filter(|t| t.2 != Kind::Plain)
            .map(|(a, b, k)| (&code[a..b], k))
            .collect()
    }

    #[test]
    fn sas_tokens() {
        use Kind::*;
        let code = "* 주석 문장;\ndata a; set b(where=(x>1));\n  y = substr(name, 1, 3) || 'it''s'd;\n\
                    /* 블록 */ %let n = &&pre&i..x;\nproc sort data=a; by y; run;\n%macro m; %mend;";
        assert_eq!(
            kinds(code),
            [
                ("* 주석 문장;", Comment),
                ("data", Keyword),
                ("set", Keyword),
                ("where", Keyword),
                ("1", Number),
                ("substr", Function),
                ("1", Number),
                ("3", Number),
                ("'it''s'd", Str),
                ("/* 블록 */", Comment),
                ("%let", Macro),
                ("&&pre", Macro),
                ("&i.", Macro),
                ("proc", Keyword),
                ("sort", Function),
                ("data", Keyword),
                ("by", Keyword),
                ("run", Keyword),
                ("%macro", Macro),
                ("%mend", Macro),
            ]
        );
        // 문장 중간의 * 는 곱셈
        assert_eq!(kinds("x = a * b;"), []);
        // 모든 바이트를 빠짐없이 덮는다 (커서 위치 보존)
        let t = tokenize(code);
        assert_eq!(t.first().unwrap().0, 0);
        assert_eq!(t.last().unwrap().1, code.len());
        assert!(t.windows(2).all(|w| w[0].1 == w[1].0));
    }
}
