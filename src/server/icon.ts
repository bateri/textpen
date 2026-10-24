// Written by tools/icon.mjs; change the shapes there and run it again.

import type { Icon } from "@pencere/sdk";

/**
 * textpen's icon, the mark its view shows, for a dark ground and a light one (an icon's `theme`),
 * 32 and 64 pixels a side, as PNGs in `data:` URIs: a terminal draws an icon only from the
 * bytes it is given, and never fetches one.
 */
export const ICONS: Icon[] = [
  {
    src: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAABI0lEQVR42u2WsUoDQRRFtwpBoojYhBQ2BkEUrMQmsKWllZBbJRD8CUt/wX8QLG8+wTJlipQpLFKk8PYWuoG3kCwGcWZnZ5EUp917eNz3ZhMRSUySugk0ROwHprFN4FrESMRDYEaWtSHQqSC4SGddoBtBoLsusCdiWGH40DI3OnAkoiciDUzPsuq5hjuBWgqcieg7ci+i7SOwauzYk1cRJ64CTyUIjO38Ogk8liTQdxU4zTrw4hn+LOLAp4RNEZeOnBef3391Bw5F3GYjPo4h0BLxLuJLxMJkKhVILTwnLUNgIGImYv4DExE3IQUusp3/LHy0yIeNPojA4JfwnKuYE1jaTQjWgTsR0y0deAvdgVpswV9o2sZEuwP5MUp9wnf/hCu+AZnippR9ikNKAAAAAElFTkSuQmCC",
    mimeType: "image/png",
    sizes: ["32x32"],
    theme: "dark",
  },
  {
    src: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAAB70lEQVR42u3YMUsDMRQH8FIVCorgchRxaKHVIoJOIthBqIWKFBxE8OFgRdyEDo4iBYeOjiJuLh1EkNexgx/BwQ/gR/CBg4ODGsgtxzU96JlLLm/407H9/2jykmQIIeNyMgzAAAzAAEMyQQgeIcxbHk92iQyQJYR1QmgRwllK0pKdslEAaikqHkxtFEAhxeX9FFQAdQcA6iqApgMATRVAwwGAhgqg7ABAWQWQTfkyaAZHYdgYnErpKKzJbpFPgtOEUCSERctTlF34LsAADMAADMAADMAADBA3wNLfZ4cQngihrzkPhHBKCLmkADYI4TmB4sHcEsKsbgDxhT0Dyvtp6wbYNah8Xy7BnE6Ac8MAREo6AU4MBPB0AqwaVv4uiSlwbRDAZhIAYtO5MmDz20n6JLhMCAeEcKg5YhLN8VGYARiAAWwD2JYXGJE91wC6hPATyL0rAGsh5f1suQBwoQDouADQsQWgIjeoV0J4j5iBfJTI2Q6wTwhfih86Km+EkLcVoDRmeT99WwFuYijvp2IjwEuMAMeuAxzZCNCNEaBkI8ACIXzGUL5n8xisEsLHGOXFMpqx/SCUlw+eA1koSh7lup9Mw0nQ+aMwX4b+KSuE8D0EoOrKg0g7pHzXtScx8TByKf/2VeJHUQZgAAZggGTyC+TmncQ8RqX5AAAAAElFTkSuQmCC",
    mimeType: "image/png",
    sizes: ["64x64"],
    theme: "dark",
  },
  {
    src: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAABI0lEQVR42u2WsUoDQRRFtwrhoiJiIylsDAFRsBKbwJYprYTALRII+YmU/oL/4MU2n2BpaWFpkSKFhR9gYTbwFpLFEJzZ2VlCitPuPTzue7OJiCQmSd0EGiIOA9PYJHArYiRiHJiRZa0JtCoILtJaFWhHEGivCkDEsMLwoWWudeBERFdEGpiuZdVzDfcCtRToiOg78iDizEdg2dipJy8izl0FHksQmNr5dRKYlCTQdxW4yDrw7Bn+JOLIp4RNEdeOXBaf3526A8cietmIT2MIHIiYifgVMTeZSgVSC89JyxAYiPgQ8fkHbyLuQgpcZTv/U/hokW8bfRCBwZbwnJuYE/iymxCsA/ci3jd04DV0B2qxBf+haRsT7Q7kxyj1Cd//Ey5ZAJvx7lg+jxf3AAAAAElFTkSuQmCC",
    mimeType: "image/png",
    sizes: ["32x32"],
    theme: "light",
  },
  {
    src: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAAB7klEQVR42u3YMUsDMRQH8FIVylMEl1LEoYVWiwg6iWAH4SxUpOAggvAGK+ImdHAUKTh0dBRxc+mDIggdO/gRHPwAfgQHBwcHNZBbjmt60DOXXN7wp2P7/9HkJckQQsblZBiAARiAAUZkihDyhLBoefKyS2SALCFsEkKLEM5TkpbslI0C4KWoeDDeOIBiisv7KaoA6g4A1FUATQcAmiqAhgMADRVAxQGAigogm/Jl0AyOwrAxOJPSUejJbpFPgrOEUCKEZctTkl34LsAADMAADMAADMAADBA3wMrfZ4cQnghhoDmPhHBGCLmkALYI4TmB4sHcEcK8bgDxhT0Dyvtp6wbYN6j8QC7BnE6AC8MARMo6AU4NBMjrBFg3rPx9ElPgxiCA7SQAxKZzbcDmt5f0SXCVEI4I4VhzxCRa4KMwAzAAA9gGsCsvMCIHrgF0CeEnkAdXADZCyvvZcQHgUgHQcQGgYwtAVW5Qr4TwHjFD+SiRsx3gkBC+FD90XN4IoWArQHnC8n4GtgLcxlDeT9VGgJcYAU5cB0AbAboxApRtBFgihM8YyvdsHoM1QviYoLxYRnO2H4QK8sFzKAtFSV+u++k0nASdPwrzZeifskYI3yMAaq48iLRDynddexITDyNX8m9fI34UZQAGYAAGSCa/8vW84w+5v0sAAAAASUVORK5CYII=",
    mimeType: "image/png",
    sizes: ["64x64"],
    theme: "light",
  },
];
