import { Globe } from "lucide-react";

import AE from "country-flag-icons/react/3x2/AE";
import AR from "country-flag-icons/react/3x2/AR";
import AT from "country-flag-icons/react/3x2/AT";
import AU from "country-flag-icons/react/3x2/AU";
import BE from "country-flag-icons/react/3x2/BE";
import BR from "country-flag-icons/react/3x2/BR";
import CA from "country-flag-icons/react/3x2/CA";
import CH from "country-flag-icons/react/3x2/CH";
import CN from "country-flag-icons/react/3x2/CN";
import CZ from "country-flag-icons/react/3x2/CZ";
import DE from "country-flag-icons/react/3x2/DE";
import DK from "country-flag-icons/react/3x2/DK";
import ES from "country-flag-icons/react/3x2/ES";
import FI from "country-flag-icons/react/3x2/FI";
import FR from "country-flag-icons/react/3x2/FR";
import GB from "country-flag-icons/react/3x2/GB";
import HK from "country-flag-icons/react/3x2/HK";
import ID from "country-flag-icons/react/3x2/ID";
import IE from "country-flag-icons/react/3x2/IE";
import IL from "country-flag-icons/react/3x2/IL";
import IN from "country-flag-icons/react/3x2/IN";
import IT from "country-flag-icons/react/3x2/IT";
import JP from "country-flag-icons/react/3x2/JP";
import KR from "country-flag-icons/react/3x2/KR";
import MO from "country-flag-icons/react/3x2/MO";
import MX from "country-flag-icons/react/3x2/MX";
import MY from "country-flag-icons/react/3x2/MY";
import NL from "country-flag-icons/react/3x2/NL";
import NO from "country-flag-icons/react/3x2/NO";
import NZ from "country-flag-icons/react/3x2/NZ";
import PH from "country-flag-icons/react/3x2/PH";
import PL from "country-flag-icons/react/3x2/PL";
import PT from "country-flag-icons/react/3x2/PT";
import RU from "country-flag-icons/react/3x2/RU";
import SA from "country-flag-icons/react/3x2/SA";
import SE from "country-flag-icons/react/3x2/SE";
import SG from "country-flag-icons/react/3x2/SG";
import TH from "country-flag-icons/react/3x2/TH";
import TR from "country-flag-icons/react/3x2/TR";
import TW from "country-flag-icons/react/3x2/TW";
import UA from "country-flag-icons/react/3x2/UA";
import US from "country-flag-icons/react/3x2/US";
import VN from "country-flag-icons/react/3x2/VN";
import ZA from "country-flag-icons/react/3x2/ZA";

import { cn } from "@/lib/utils";

type FlagComponent = (props: React.HTMLAttributes<SVGElement>) => React.JSX.Element;

const FLAG_BY_COUNTRY: Record<string, FlagComponent> = {
  阿根廷: AR,
  阿联酋: AE,
  奥地利: AT,
  澳大利亚: AU,
  澳门: MO,
  巴西: BR,
  比利时: BE,
  波兰: PL,
  丹麦: DK,
  德国: DE,
  俄罗斯: RU,
  法国: FR,
  芬兰: FI,
  韩国: KR,
  荷兰: NL,
  加拿大: CA,
  捷克: CZ,
  马来西亚: MY,
  美国: US,
  墨西哥: MX,
  南非: ZA,
  挪威: NO,
  葡萄牙: PT,
  日本: JP,
  瑞士: CH,
  瑞典: SE,
  沙特阿拉伯: SA,
  台湾: TW,
  泰国: TH,
  土耳其: TR,
  乌克兰: UA,
  西班牙: ES,
  香港: HK,
  新加坡: SG,
  新西兰: NZ,
  以色列: IL,
  意大利: IT,
  印度: IN,
  印度尼西亚: ID,
  英国: GB,
  越南: VN,
  中国: CN,
  爱尔兰: IE,
  菲律宾: PH,
};

interface FlagProps {
  country: string;
  className?: string;
}

export function Flag({ country, className }: FlagProps) {
  const FlagIcon = FLAG_BY_COUNTRY[country];

  if (!FlagIcon) {
    return (
      <Globe
        aria-label={country}
        className={cn("text-muted-foreground", className)}
      />
    );
  }

  return (
    <FlagIcon
      aria-label={country}
      className={cn("object-cover", className)}
    />
  );
}
