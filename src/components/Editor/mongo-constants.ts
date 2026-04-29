// SPDX-License-Identifier: Apache-2.0

export const MONGO_TEMPLATES = {
  find: `db.collection.find({
  // query filter
})`,
  findOne: `db.collection.findOne({
  // query filter
})`,
  aggregate: `db.collection.aggregate([
  { $match: { } },
  { $group: { _id: "$field", count: { $sum: 1 } } }
])`,
  aggregateTopN: `db.collection.aggregate([
  { $match: { } },
  { $sort: { createdAt: -1 } },
  { $limit: 10 }
])`,
  aggregateLookup: `db.collection.aggregate([
  { $lookup: {
      from: "otherCollection",
      localField: "foreignId",
      foreignField: "_id",
      as: "joined"
  } },
  { $unwind: { path: "$joined", preserveNullAndEmptyArrays: true } }
])`,
  insertOne: `db.collection.insertOne({
  // document
})`,
  updateOne: `db.collection.updateOne(
  { /* filter */ },
  { $set: { /* update */ } }
)`,
  updateMany: `db.collection.updateMany(
  { /* filter */ },
  { $set: { /* update */ } }
)`,
  deleteOne: `db.collection.deleteOne({
  // filter
})`,
  bulkWrite: `{
  "operation": "bulkWrite",
  "database": "mydb",
  "collection": "mycollection",
  "operations": [
    { "insertOne": { "document": { "name": "alice" } } },
    { "updateOne": { "filter": { "_id": 1 }, "update": { "$set": { "active": true } }, "upsert": true } },
    { "deleteOne": { "filter": { "legacy": true } } }
  ]
}`,
  findOneAndUpdate: `{
  "operation": "findOneAndUpdate",
  "database": "mydb",
  "collection": "mycollection",
  "filter": { "_id": 1 },
  "update": { "$set": { "lastSeen": "now" } },
  "options": { "returnDocument": "after" }
}`,
  findOneAndReplace: `{
  "operation": "findOneAndReplace",
  "database": "mydb",
  "collection": "mycollection",
  "filter": { "_id": 1 },
  "replacement": { "name": "alice", "active": true },
  "options": { "returnDocument": "after" }
}`,
  findOneAndDelete: `{
  "operation": "findOneAndDelete",
  "database": "mydb",
  "collection": "mycollection",
  "filter": { "_id": 1 }
}`,
};
