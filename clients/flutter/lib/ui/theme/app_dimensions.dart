/// Design-token spacing & radii. Use these instead of ad-hoc magic numbers so
/// the whole app breathes with one rhythm.
class HermesSpacing {
  const HermesSpacing._();

  static const double xs = 4;
  static const double sm = 8;
  static const double md = 12;
  static const double lg = 16;
  static const double xl = 24;
  static const double xxl = 32;
}

/// Corner radii. Bubbles use [bubble] with an asymmetric tail corner for a
/// chat feel; everything else sticks to the [sm]/[md]/[lg]/[xl] ladder.
class HermesRadius {
  const HermesRadius._();

  static const double sm = 8;
  static const double md = 12;
  static const double lg = 16;
  static const double xl = 24;
  static const double bubble = 20;
  static const double pill = 999;
}
