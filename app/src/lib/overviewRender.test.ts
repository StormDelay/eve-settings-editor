// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
// The layout rules themselves are tested in overview_fit.rs.
import { effectiveWidth, fadeStart } from "./overviewRender.ts";
import { check } from "./test/check.ts";

// --- widths: the client's own defaults, as overview_fit.rs has them ---------
check("a stored width is used as is", effectiveWidth("VELOCITY", 45) === 45);
check("a stored width is floored at COLUMNMINSIZE", effectiveWidth("VELOCITY", 10) === 24);
check("the icon column is fixed at 22 whatever is stored", effectiveWidth("ICON", 90) === 22);
check("absent velocity is 58", effectiveWidth("VELOCITY", null) === 58);
check("absent name is 112", effectiveWidth("NAME", null) === 112);
check("absent anything else is 80", effectiveWidth("CORPORATION", null) === 80);

// The fade mask covers the whole label: opaque up to the stop, so an
// overflowing header keeps its visible part instead of vanishing.
check("a 60 px label fading over 20 px is opaque for its first two thirds", Math.abs(fadeStart(60, 20) - 2 / 3) < 1e-9);
check("a fade wider than the label starts at its left edge", fadeStart(10, 20) === 0);
