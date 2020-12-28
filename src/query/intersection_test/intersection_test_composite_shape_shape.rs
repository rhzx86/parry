use crate::bounding_volume::SimdAABB;
use crate::math::{Isometry, Real, SimdReal, Vector, SIMD_WIDTH};
use crate::partitioning::{SimdBestFirstVisitStatus, SimdBestFirstVisitor};
use crate::query::QueryDispatcher;
use crate::shape::{Shape, SimdCompositeShape};
use crate::utils::IsometryOpt;
use simba::simd::{SimdBool as _, SimdPartialOrd, SimdValue};

/// Intersection test between a composite shape (`Mesh`, `Compound`) and any other shape.
pub fn intersection_test_composite_shape_shape<D: ?Sized, G1: ?Sized>(
    dispatcher: &D,
    pos12: &Isometry<Real>,
    g1: &G1,
    g2: &dyn Shape,
) -> bool
where
    D: QueryDispatcher,
    G1: SimdCompositeShape,
{
    let mut visitor = CompositeShapeAgainstAnyIntersectionVisitor::new(dispatcher, pos12, g1, g2);

    g1.quadtree()
        .traverse_best_first(&mut visitor)
        .map(|e| e.1)
        .unwrap_or(false)
}

/// Proximity between a shape and a composite (`Mesh`, `Compound`) shape.
pub fn intersection_test_shape_composite_shape<D: ?Sized, G2: ?Sized>(
    dispatcher: &D,
    pos12: &Isometry<Real>,
    g1: &dyn Shape,
    g2: &G2,
) -> bool
where
    D: QueryDispatcher,
    G2: SimdCompositeShape,
{
    intersection_test_composite_shape_shape(dispatcher, &pos12.inverse(), g2, g1)
}

struct CompositeShapeAgainstAnyIntersectionVisitor<'a, D: ?Sized, G1: ?Sized + 'a> {
    msum_shift: Vector<SimdReal>,
    msum_margin: Vector<SimdReal>,

    dispatcher: &'a D,
    pos12: &'a Isometry<Real>,
    g1: &'a G1,
    g2: &'a dyn Shape,
}

impl<'a, D: ?Sized, G1: ?Sized> CompositeShapeAgainstAnyIntersectionVisitor<'a, D, G1>
where
    D: QueryDispatcher,
    G1: SimdCompositeShape,
{
    pub fn new(
        dispatcher: &'a D,
        pos12: &'a Isometry<Real>,
        g1: &'a G1,
        g2: &'a dyn Shape,
    ) -> CompositeShapeAgainstAnyIntersectionVisitor<'a, D, G1> {
        let ls_aabb2 = g2.compute_aabb(&pos12);

        CompositeShapeAgainstAnyIntersectionVisitor {
            dispatcher,
            msum_shift: Vector::splat(-ls_aabb2.center().coords),
            msum_margin: Vector::splat(ls_aabb2.half_extents()),
            pos12,
            g1,
            g2,
        }
    }
}

impl<'a, D: ?Sized, G1: ?Sized> SimdBestFirstVisitor<u32, SimdAABB>
    for CompositeShapeAgainstAnyIntersectionVisitor<'a, D, G1>
where
    D: QueryDispatcher,
    G1: SimdCompositeShape,
{
    type Result = bool;

    fn visit(
        &mut self,
        best: Real,
        bv: &SimdAABB,
        data: Option<[Option<&u32>; SIMD_WIDTH]>,
    ) -> SimdBestFirstVisitStatus<Self::Result> {
        // Compute the minkowski sum of the two AABBs.
        let msum = SimdAABB {
            mins: bv.mins + self.msum_shift + (-self.msum_margin),
            maxs: bv.maxs + self.msum_shift + self.msum_margin,
        };
        let dist = msum.distance_to_origin();
        let mask = dist.simd_lt(SimdReal::splat(best));

        if let Some(data) = data {
            let bitmask = mask.bitmask();
            let mut found_intersection = false;

            for ii in 0..SIMD_WIDTH {
                if (bitmask & (1 << ii)) != 0 && data[ii].is_some() {
                    self.g1
                        .map_part_at(*data[ii].unwrap(), &mut |part_pos1, g1| {
                            found_intersection = self.dispatcher.intersection_test(
                                &part_pos1.inv_mul(self.pos12),
                                g1,
                                self.g2,
                            ) == Ok(true);
                        });

                    if found_intersection {
                        return SimdBestFirstVisitStatus::ExitEarly(Some(true));
                    }
                }
            }
        }

        SimdBestFirstVisitStatus::MaybeContinue {
            weights: dist,
            mask,
            results: [None; SIMD_WIDTH],
        }
    }
}
